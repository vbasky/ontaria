//! RDFS entailment, plus OWL 2 RL when the `reasoning` feature is enabled.
//!
//! RDFS rules run as plain functions over triples. The OWL 2 RL engine is
//! `reasonable`, which is not [`Send`] or [`Sync`], so it is created inside
//! [`apply_owl_rl`] and dropped before the function returns. It is never
//! stored on [`Ontology`](crate::Ontology).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::extract::from_statements;
use crate::model::{Fact, Ontology, Statement, TermKind};
use crate::vocab::{self, RDF_TYPE, RDFS_DOMAIN, RDFS_RANGE, RDFS_SUBCLASS, RDFS_SUBPROPERTY};

/// A subject and the types entailed for it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypeFacts {
    pub subject: String,
    pub types: Vec<Fact<String>>,
}

/// Materialize RDFS subclass, subproperty, domain, and range consequences.
///
/// Asserted triples keep `inferred: false`. Triples added here set
/// `inferred: true`.
pub fn apply_rdfs(ontology: &Ontology) -> Ontology {
    let subclass = ancestor_map(ontology, RDFS_SUBCLASS);
    let subproperty = ancestor_map(ontology, RDFS_SUBPROPERTY);
    let mut statements = ontology.statements.clone();
    let mut seen = statement_keys(&statements);

    let snapshot = statements.clone();
    for statement in &snapshot {
        if statement.predicate == RDF_TYPE && statement.kind == TermKind::Iri {
            for parent in subclass.get(&statement.object).into_iter().flatten() {
                push_inferred(
                    &mut statements,
                    &mut seen,
                    &statement.subject,
                    RDF_TYPE,
                    parent,
                    TermKind::Iri,
                );
            }
        }
        if let Some(parents) = subproperty.get(&statement.predicate) {
            for parent in parents {
                push_inferred(
                    &mut statements,
                    &mut seen,
                    &statement.subject,
                    parent,
                    &statement.object,
                    statement.kind,
                );
            }
        }
        if let Some(domains) = objects_of(ontology, &statement.predicate, RDFS_DOMAIN) {
            for domain in domains {
                push_inferred(
                    &mut statements,
                    &mut seen,
                    &statement.subject,
                    RDF_TYPE,
                    &domain,
                    TermKind::Iri,
                );
            }
        }
        if statement.kind == TermKind::Iri
            && let Some(ranges) = objects_of(ontology, &statement.predicate, RDFS_RANGE)
        {
            for range in ranges {
                if range.starts_with(vocab::XSD) {
                    continue;
                }
                push_inferred(
                    &mut statements,
                    &mut seen,
                    &statement.object,
                    RDF_TYPE,
                    &range,
                    TermKind::Iri,
                );
            }
        }
    }

    for (child, parents) in &subclass {
        for parent in parents {
            push_inferred(
                &mut statements,
                &mut seen,
                child,
                RDFS_SUBCLASS,
                parent,
                TermKind::Iri,
            );
        }
    }

    let mut entailed = from_statements(&ontology.uri, &ontology.format, &statements);
    entailed.warnings = ontology.warnings.clone();
    entailed.metadata = ontology.metadata.clone();
    entailed.declared_prefixes = ontology.declared_prefixes.clone();
    entailed
}

/// Types of `term`, each marked asserted or inferred.
pub fn types_of(ontology: &Ontology, term: &str) -> Vec<Fact<String>> {
    let resolved = ontology.resolve_prefix(term);
    let mut facts = Vec::new();
    for statement in &ontology.statements {
        if statement.predicate == RDF_TYPE
            && (statement.subject == term || statement.subject == resolved)
            && statement.kind == TermKind::Iri
        {
            facts.push(Fact {
                value: statement.object.clone(),
                asserted: !statement.inferred,
            });
        }
    }
    facts.sort_by(|left, right| left.value.cmp(&right.value));
    facts.dedup();
    facts
}

/// OWL 2 RL materialization through `reasonable`.
///
/// The reasoner is not thread-safe. This function owns it for the duration
/// of the call and returns a plain [`Ontology`].
#[cfg(feature = "reasoning")]
pub fn apply_owl_rl(ontology: &Ontology) -> Result<Ontology> {
    use reasonable::reasoner::Reasoner;

    use crate::extract::statement_to_triple;

    let input: Vec<oxrdf::Triple> = ontology
        .statements
        .iter()
        .filter_map(statement_to_triple)
        .collect();
    let input_keys: BTreeSet<_> = input.iter().map(triple_key).collect();
    let mut reasoner = Reasoner::new();
    reasoner.load_triples(input);
    reasoner.reason();
    let mut statements = ontology.statements.clone();
    let mut seen: std::collections::BTreeSet<(String, String, String)> = ontology
        .statements
        .iter()
        .map(|statement| {
            (
                statement.subject.clone(),
                statement.predicate.clone(),
                statement.object.clone(),
            )
        })
        .collect();
    for triple in reasoner.get_triples() {
        let key = triple_key(&triple);
        if input_keys.contains(&key) || !seen.insert(key) {
            continue;
        }
        if let Some(mut statement) = triple_to_statement(&triple) {
            statement.inferred = true;
            statements.push(statement);
        }
    }
    let mut entailed = from_statements(&ontology.uri, &ontology.format, &statements);
    entailed.warnings = ontology.warnings.clone();
    entailed.metadata = ontology.metadata.clone();
    Ok(entailed)
}

/// Same entry point when the feature is off, so callers can match on [`Error::Feature`].
#[cfg(not(feature = "reasoning"))]
pub fn apply_owl_rl(ontology: &Ontology) -> Result<Ontology> {
    let _ = ontology;
    Err(crate::error::Error::feature(
        "reasoning",
        "rebuild ontaria with `--features reasoning` to run OWL 2 RL",
    ))
}

#[cfg(feature = "reasoning")]
fn triple_key(triple: &oxrdf::Triple) -> (String, String, String) {
    (
        triple.subject.to_string(),
        triple.predicate.to_string(),
        triple.object.to_string(),
    )
}

#[cfg(feature = "reasoning")]
fn triple_to_statement(triple: &oxrdf::Triple) -> Option<Statement> {
    let (object, kind, datatype, language) = match &triple.object {
        oxrdf::Term::NamedNode(node) => (node.as_str().to_string(), TermKind::Iri, None, None),
        oxrdf::Term::BlankNode(node) => {
            (format!("_:{}", node.as_str()), TermKind::Blank, None, None)
        }
        oxrdf::Term::Literal(literal) => (
            literal.value().to_string(),
            TermKind::Literal,
            Some(literal.datatype().as_str().to_string()),
            literal.language().map(str::to_string),
        ),
        #[allow(unreachable_patterns)]
        _ => return None,
    };
    let subject = match &triple.subject {
        oxrdf::NamedOrBlankNode::NamedNode(node) => node.as_str().to_string(),
        oxrdf::NamedOrBlankNode::BlankNode(node) => format!("_:{}", node.as_str()),
    };
    Some(Statement {
        subject,
        predicate: triple.predicate.as_str().to_string(),
        object,
        kind,
        datatype,
        language,
        graph: None,
        inferred: false,
    })
}

fn statement_keys(statements: &[Statement]) -> BTreeSet<(String, String, String, String)> {
    statements
        .iter()
        .map(|statement| {
            (
                statement.subject.clone(),
                statement.predicate.clone(),
                statement.object.clone(),
                statement.graph.clone().unwrap_or_default(),
            )
        })
        .collect()
}

fn push_inferred(
    statements: &mut Vec<Statement>,
    seen: &mut BTreeSet<(String, String, String, String)>,
    subject: &str,
    predicate: &str,
    object: &str,
    kind: TermKind,
) {
    let key = (
        subject.to_string(),
        predicate.to_string(),
        object.to_string(),
        String::new(),
    );
    if !seen.insert(key) {
        return;
    }
    statements.push(Statement {
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        object: object.to_string(),
        kind,
        datatype: None,
        language: None,
        graph: None,
        inferred: true,
    });
}

fn objects_of(ontology: &Ontology, subject: &str, predicate: &str) -> Option<Vec<String>> {
    let values: Vec<String> = ontology
        .statements
        .iter()
        .filter(|statement| {
            statement.subject == subject
                && statement.predicate == predicate
                && statement.kind == TermKind::Iri
                && !statement.inferred
        })
        .map(|statement| statement.object.clone())
        .collect();
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

/// Every node reachable by `predicate`, including the direct targets.
/// [`push_inferred`] skips triples that were already asserted.
fn ancestor_map(ontology: &Ontology, predicate: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut direct: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for statement in &ontology.statements {
        if statement.predicate == predicate
            && statement.kind == TermKind::Iri
            && !statement.inferred
        {
            direct
                .entry(statement.subject.clone())
                .or_default()
                .insert(statement.object.clone());
        }
    }
    let mut closure: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for start in direct.keys() {
        let mut pending: Vec<String> = direct.get(start).into_iter().flatten().cloned().collect();
        let mut seen = BTreeSet::new();
        while let Some(next) = pending.pop() {
            if !seen.insert(next.clone()) {
                continue;
            }
            if let Some(more) = direct.get(&next) {
                pending.extend(more.iter().cloned());
            }
        }
        if !seen.is_empty() {
            closure.insert(start.clone(), seen);
        }
    }
    closure
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;

    #[test]
    fn infers_a_parent_type_and_keeps_the_asserted_one() {
        let ontology = parse_document(
            br#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            ex:Agent a owl:Class .
            ex:Person a owl:Class ; rdfs:subClassOf ex:Agent .
            ex:ada a ex:Person .
            "#,
            "http://example.org/",
        )
        .unwrap();
        let entailed = apply_rdfs(&ontology);
        let types = types_of(&entailed, "http://example.org/ada");
        let person = types
            .iter()
            .find(|fact| fact.value.ends_with("Person"))
            .unwrap();
        let agent = types
            .iter()
            .find(|fact| fact.value.ends_with("Agent"))
            .unwrap();
        assert!(person.asserted);
        assert!(!agent.asserted);
    }

    #[test]
    fn owl_rl_reports_a_missing_feature_unless_enabled() {
        let ontology = Ontology::new("http://example.org/");
        let result = apply_owl_rl(&ontology);
        #[cfg(not(feature = "reasoning"))]
        {
            assert!(matches!(
                result,
                Err(crate::error::Error::Feature {
                    feature: "reasoning",
                    ..
                })
            ));
        }
        #[cfg(feature = "reasoning")]
        {
            assert!(result.is_ok());
        }
    }
}
