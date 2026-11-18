//! Static checks on an ontology document.
//!
//! Cycles and restrictions that name an undefined term are errors. Missing
//! labels, missing domain or range, unused declared prefixes, deprecated
//! terms that are still referenced, and IRIs outside the ontology namespace
//! are warnings.

use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::model::{Ontology, PropertyRef};
use crate::validate::{Severity, ValidationIssue, ValidationReport};
use crate::vocab::{self, local_name};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LintOptions {
    /// When set, warnings are reported as errors.
    pub deny_warnings: bool,
}

/// Lint `ontology`. Built-in prefixes that ontaria inserts are not unused.
pub fn lint(ontology: &Ontology, options: &LintOptions) -> ValidationReport {
    let mut issues = Vec::new();
    for class in ontology.classes.values() {
        if class.label.is_empty() {
            issues.push(warn(&class.uri, "missing_label", "class has no label"));
        }
        if class.comment.is_empty() && class.description.is_empty() {
            issues.push(warn(
                &class.uri,
                "missing_comment",
                "class has no comment or description",
            ));
        }
        if outside_namespace(ontology, &class.uri) {
            issues.push(warn(
                &class.uri,
                "namespace",
                "class IRI is outside the ontology namespace",
            ));
        }
        for restriction in &class.properties {
            check_restriction(ontology, &class.uri, restriction, &mut issues);
        }
    }
    for property in ontology.properties.values() {
        if property.label.is_empty() {
            issues.push(warn(
                &property.uri,
                "missing_label",
                "property has no label",
            ));
        }
        if property.comment.is_empty() && property.description.is_empty() {
            issues.push(warn(
                &property.uri,
                "missing_comment",
                "property has no comment or description",
            ));
        }
        if property.domain.is_empty() && property.domain_expressions.is_empty() {
            issues.push(warn(
                &property.uri,
                "missing_domain",
                "property has no domain",
            ));
        }
        if property.range.is_empty() && property.range_expressions.is_empty() {
            issues.push(warn(
                &property.uri,
                "missing_range",
                "property has no range",
            ));
        }
        if outside_namespace(ontology, &property.uri) {
            issues.push(warn(
                &property.uri,
                "namespace",
                "property IRI is outside the ontology namespace",
            ));
        }
    }
    for cycle in subclass_cycles(ontology) {
        issues.push(ValidationIssue {
            path: cycle.clone(),
            term: cycle,
            rule: "cycle".to_string(),
            severity: Severity::Error,
            message: "subClassOf cycle".to_string(),
        });
    }
    for prefix in unused_prefixes(ontology) {
        issues.push(warn(
            &prefix,
            "unused_prefix",
            &format!("declared prefix {prefix} is unused"),
        ));
    }
    for (term, message) in deprecated_references(ontology) {
        issues.push(warn(&term, "deprecated", &message));
    }
    if options.deny_warnings {
        for issue in &mut issues {
            issue.severity = Severity::Error;
        }
    }
    issues.sort_by(|left, right| {
        left.rule
            .cmp(&right.rule)
            .then(left.term.cmp(&right.term))
            .then(left.message.cmp(&right.message))
    });
    issues.dedup();
    ValidationReport { issues }
}

fn warn(term: &str, rule: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        path: term.to_string(),
        term: term.to_string(),
        rule: rule.to_string(),
        severity: Severity::Warning,
        message: message.to_string(),
    }
}

fn outside_namespace(ontology: &Ontology, iri: &str) -> bool {
    let base = namespace_of(&ontology.uri);
    if base.is_empty() {
        return false;
    }
    !iri.starts_with(&base) && !vocab::is_builtin_namespace(iri)
}

fn namespace_of(uri: &str) -> String {
    if let Some(index) = uri.rfind(['#', '/']) {
        uri[..=index].to_string()
    } else {
        String::new()
    }
}

fn check_restriction(
    ontology: &Ontology,
    class_uri: &str,
    restriction: &PropertyRef,
    issues: &mut Vec<ValidationIssue>,
) {
    if !ontology.properties.contains_key(&restriction.uri)
        && !vocab::is_builtin_namespace(&restriction.uri)
    {
        issues.push(ValidationIssue {
            path: class_uri.to_string(),
            term: restriction.uri.clone(),
            rule: "undefined_term".to_string(),
            severity: Severity::Error,
            message: format!(
                "restriction on {} names undefined property {}",
                local_name(class_uri),
                restriction.uri
            ),
        });
    }
    for target in [
        restriction.on_class.as_deref(),
        restriction.some_values_from.as_deref(),
        restriction.all_values_from.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if ontology.classes.contains_key(target) || vocab::is_builtin_namespace(target) {
            continue;
        }
        issues.push(ValidationIssue {
            path: class_uri.to_string(),
            term: target.to_string(),
            rule: "undefined_term".to_string(),
            severity: Severity::Error,
            message: format!("restriction target {target} is not defined"),
        });
    }
}

fn subclass_cycles(ontology: &Ontology) -> Vec<String> {
    let mut cycles = Vec::new();
    for start in ontology.classes.keys() {
        let mut stack = VecDeque::from([start.clone()]);
        let mut seen = BTreeSet::new();
        while let Some(uri) = stack.pop_front() {
            if !seen.insert(uri.clone()) {
                if &uri == start {
                    cycles.push(start.clone());
                }
                continue;
            }
            if let Some(class) = ontology.classes.get(&uri) {
                stack.extend(class.parents.iter().cloned());
            }
        }
    }
    cycles.sort();
    cycles.dedup();
    cycles
}

fn unused_prefixes(ontology: &Ontology) -> Vec<String> {
    let mut used = BTreeSet::new();
    let texts = ontology
        .classes
        .keys()
        .chain(ontology.properties.keys())
        .chain(ontology.individuals.keys())
        .chain(ontology.concepts.keys());
    for iri in texts {
        if let Some(prefix) = prefix_for(ontology, iri) {
            used.insert(prefix);
        }
    }
    ontology
        .declared_prefixes
        .keys()
        .filter(|prefix| !used.contains(*prefix))
        .cloned()
        .collect()
}

fn prefix_for(ontology: &Ontology, iri: &str) -> Option<String> {
    ontology
        .declared_prefixes
        .iter()
        .find(|(_, namespace)| iri.starts_with(namespace.as_str()))
        .map(|(prefix, _)| prefix.clone())
}

fn deprecated_references(ontology: &Ontology) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let deprecated_classes: BTreeSet<_> = ontology
        .classes
        .values()
        .filter(|class| class.deprecated)
        .map(|class| class.uri.clone())
        .collect();
    let deprecated_properties: BTreeSet<_> = ontology
        .properties
        .values()
        .filter(|property| property.deprecated)
        .map(|property| property.uri.clone())
        .collect();
    for class in ontology.classes.values() {
        for parent in &class.parents {
            if deprecated_classes.contains(parent) {
                found.push((
                    parent.clone(),
                    format!("{} subclasses deprecated {parent}", class.uri),
                ));
            }
        }
        for restriction in &class.properties {
            if deprecated_properties.contains(&restriction.uri) {
                found.push((
                    restriction.uri.clone(),
                    format!("{} restricts deprecated {}", class.uri, restriction.uri),
                ));
            }
        }
    }
    for property in ontology.properties.values() {
        for domain in property.domain.iter().chain(property.range.iter()) {
            if deprecated_classes.contains(domain) {
                found.push((
                    domain.clone(),
                    format!("{} references deprecated {domain}", property.uri),
                ));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;

    #[test]
    fn reports_a_cycle_and_a_missing_label() {
        let ontology = parse_document(
            br#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix unused: <http://example.org/unused/> .
            ex: a owl:Ontology .
            ex:A a owl:Class ; rdfs:subClassOf ex:B .
            ex:B a owl:Class ; rdfs:label "B" ; rdfs:comment "b" ; rdfs:subClassOf ex:A .
            ex:name a owl:DatatypeProperty .
            "#,
            "http://example.org/",
        )
        .unwrap();
        let report = lint(&ontology, &LintOptions::default());
        assert!(report.issues.iter().any(|issue| issue.rule == "cycle"));
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.rule == "missing_label" && issue.term.ends_with("A"))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.rule == "unused_prefix")
        );
        assert!(report.has_errors());
        let denied = lint(
            &ontology,
            &LintOptions {
                deny_warnings: true,
            },
        );
        assert!(
            denied
                .issues
                .iter()
                .all(|issue| issue.severity == Severity::Error)
        );
    }
}
