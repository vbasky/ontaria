//! Turtle to the class/property model.
//!
//! Triples are indexed once by subject. Extraction then walks that index.

use std::collections::HashMap;
use std::io::Read;

use oxrdf::{BlankNode, Literal, NamedOrBlankNode, Term, Triple};
use oxttl::TurtleParser;

use crate::error::{Error, Result};
use crate::model::{Class, Ontology, Property, PropertyRef, PropertyType, push_unique};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_QUAL_CARD: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const OWL_MIN_QUAL: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const OWL_MAX_QUAL: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_DEPRECATED: &str = "http://www.w3.org/2002/07/owl#deprecated";
const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
const DCTERMS_DESCRIPTION: &str = "http://purl.org/dc/terms/description";
const SCHEMA_DOMAIN: &[&str] = &[
    "https://schema.org/domainIncludes",
    "http://schema.org/domainIncludes",
];
const SCHEMA_RANGE: &[&str] = &[
    "https://schema.org/rangeIncludes",
    "http://schema.org/rangeIncludes",
];

/// Parse Turtle from any reader. `base_uri` resolves relative IRIs and is
/// stored as the ontology URI.
pub fn parse_turtle(reader: impl Read, base_uri: &str) -> Result<Ontology> {
    let mut built = TurtleParser::new();
    if !base_uri.is_empty() {
        built = built
            .with_base_iri(base_uri)
            .map_err(|err| Error::BaseIri {
                base: base_uri.to_string(),
                reason: err.to_string(),
            })?;
    }
    let mut parser = built.for_reader(reader);
    let mut triples = Vec::new();
    for triple in &mut parser {
        triples.push(triple.map_err(|err| Error::Parse(err.to_string()))?);
    }

    let mut ontology = Ontology::new(base_uri);
    for (prefix, iri) in parser.prefixes() {
        if !prefix.is_empty() {
            ontology
                .namespaces
                .insert(prefix.to_string(), iri.to_string());
        }
    }

    let index = Index::build(triples);
    extract_classes(&index, &mut ontology);
    extract_properties(&index, &mut ontology);
    link_hierarchy(&index, &mut ontology);
    extract_restrictions(&index, &mut ontology);
    extract_imports(&index, &mut ontology);
    Ok(ontology)
}

struct Index {
    by_subject: HashMap<NamedOrBlankNode, Vec<Triple>>,
}

impl Index {
    fn build(triples: Vec<Triple>) -> Self {
        let mut by_subject: HashMap<NamedOrBlankNode, Vec<Triple>> = HashMap::new();
        for triple in triples {
            by_subject
                .entry(triple.subject.clone())
                .or_default()
                .push(triple);
        }
        Self { by_subject }
    }

    fn objects<'a>(&'a self, subject: &NamedOrBlankNode, predicate: &str) -> Vec<&'a Term> {
        self.by_subject
            .get(subject)
            .map(|triples| {
                triples
                    .iter()
                    .filter(|triple| triple.predicate.as_str() == predicate)
                    .map(|triple| &triple.object)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn iris(&self, subject: &NamedOrBlankNode, predicate: &str) -> Vec<String> {
        self.objects(subject, predicate)
            .into_iter()
            .filter_map(term_iri)
            .collect()
    }

    fn literal(&self, subject: &NamedOrBlankNode, predicate: &str) -> String {
        preferred_literal(&self.objects(subject, predicate))
    }
}

fn extract_classes(index: &Index, ontology: &mut Ontology) {
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(node) = subject else {
            continue;
        };
        if !typed_as(triples, OWL_CLASS) && !typed_as(triples, RDFS_CLASS) {
            continue;
        }
        let uri = node.as_str().to_string();
        let class = Class {
            uri: uri.clone(),
            ontology_uri: ontology.uri.clone(),
            label: index.literal(subject, RDFS_LABEL),
            comment: index.literal(subject, RDFS_COMMENT),
            description: index.literal(subject, DCTERMS_DESCRIPTION),
            parents: Vec::new(),
            children: Vec::new(),
            properties: Vec::new(),
            deprecated: is_deprecated(index, subject),
        };
        ontology.classes.insert(uri, class);
    }
}

fn extract_properties(index: &Index, ontology: &mut Ontology) {
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(node) = subject else {
            continue;
        };
        let Some(kind) = property_type(triples) else {
            continue;
        };
        let uri = node.as_str().to_string();
        let mut domain = index.iris(subject, RDFS_DOMAIN);
        let mut range = index.iris(subject, RDFS_RANGE);
        for predicate in SCHEMA_DOMAIN {
            for iri in index.iris(subject, predicate) {
                push_unique(&mut domain, iri);
            }
        }
        for predicate in SCHEMA_RANGE {
            for iri in index.iris(subject, predicate) {
                push_unique(&mut range, iri);
            }
        }
        ontology.properties.insert(
            uri.clone(),
            Property {
                uri,
                ontology_uri: ontology.uri.clone(),
                label: index.literal(subject, RDFS_LABEL),
                comment: index.literal(subject, RDFS_COMMENT),
                description: index.literal(subject, DCTERMS_DESCRIPTION),
                domain,
                range,
                kind,
            },
        );
    }
}

fn link_hierarchy(index: &Index, ontology: &mut Ontology) {
    let mut edges: Vec<(String, String)> = Vec::new();
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(child) = subject else {
            continue;
        };
        for triple in triples {
            if triple.predicate.as_str() != RDFS_SUBCLASS {
                continue;
            }
            let Term::NamedNode(parent) = &triple.object else {
                continue;
            };
            edges.push((child.as_str().to_string(), parent.as_str().to_string()));
        }
    }
    for (child_uri, parent_uri) in edges {
        if let Some(child) = ontology.classes.get_mut(&child_uri) {
            push_unique(&mut child.parents, parent_uri.clone());
        }
        if let Some(parent) = ontology.classes.get_mut(&parent_uri) {
            push_unique(&mut parent.children, child_uri);
        }
    }
}

fn extract_restrictions(index: &Index, ontology: &mut Ontology) {
    let mut on_property: HashMap<BlankNode, (String, bool, String)> = HashMap::new();
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::BlankNode(blank) = subject else {
            continue;
        };
        if !typed_as(triples, OWL_RESTRICTION) {
            continue;
        }
        let Some(property) = index.iris(subject, OWL_ON_PROPERTY).into_iter().next() else {
            continue;
        };
        let (required, cardinality) = cardinality(index, subject);
        on_property.insert(blank.clone(), (property, required, cardinality));
    }

    let mut links: Vec<(String, String, bool, String)> = Vec::new();
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(class_node) = subject else {
            continue;
        };
        for triple in triples {
            if triple.predicate.as_str() != RDFS_SUBCLASS {
                continue;
            }
            let Term::BlankNode(blank) = &triple.object else {
                continue;
            };
            if let Some((property, required, cardinality)) = on_property.get(blank) {
                links.push((
                    class_node.as_str().to_string(),
                    property.clone(),
                    *required,
                    cardinality.clone(),
                ));
            }
        }
    }
    for (class_uri, property_uri, required, cardinality) in links {
        let Some(class) = ontology.classes.get_mut(&class_uri) else {
            continue;
        };
        if class
            .properties
            .iter()
            .any(|existing| existing.uri == property_uri)
        {
            continue;
        }
        class.properties.push(PropertyRef {
            uri: property_uri,
            required,
            cardinality,
            source_class: class_uri,
        });
    }
}

fn extract_imports(index: &Index, ontology: &mut Ontology) {
    // Any subject may declare owl:imports. Some files hang it off the
    // ontology IRI, others off a different resource.
    for triples in index.by_subject.values() {
        for triple in triples {
            if triple.predicate.as_str() != OWL_IMPORTS {
                continue;
            }
            if let Some(iri) = term_iri(&triple.object) {
                push_unique(&mut ontology.imports, iri);
            }
        }
    }
}

fn cardinality(index: &Index, subject: &NamedOrBlankNode) -> (bool, String) {
    let exact = first_number(index, subject, &[OWL_CARDINALITY, OWL_QUAL_CARD]);
    let min = first_number(index, subject, &[OWL_MIN, OWL_MIN_QUAL]);
    let max = first_number(index, subject, &[OWL_MAX, OWL_MAX_QUAL]);
    let required = exact.is_some_and(|value| value >= 1) || min.is_some_and(|value| value >= 1);
    let text = match (exact, min, max) {
        (Some(value), _, _) => value.to_string(),
        (None, Some(min), Some(max)) => format!("{min}..{max}"),
        (None, Some(min), None) => format!("{min}..*"),
        (None, None, Some(max)) => format!("0..{max}"),
        (None, None, None) => String::new(),
    };
    (required, text)
}

fn first_number(index: &Index, subject: &NamedOrBlankNode, predicates: &[&str]) -> Option<u64> {
    for predicate in predicates {
        for term in index.objects(subject, predicate) {
            let Term::Literal(literal) = term else {
                continue;
            };
            if let Ok(value) = literal.value().parse::<u64>() {
                return Some(value);
            }
        }
    }
    None
}

fn is_deprecated(index: &Index, subject: &NamedOrBlankNode) -> bool {
    index
        .objects(subject, OWL_DEPRECATED)
        .iter()
        .any(|term| match term {
            Term::Literal(literal) => literal.value().eq_ignore_ascii_case("true"),
            _ => false,
        })
}

/// More specific OWL property types beat a bare `rdf:Property`.
fn property_type(triples: &[Triple]) -> Option<PropertyType> {
    let mut best: Option<(u8, PropertyType)> = None;
    for triple in triples {
        if triple.predicate.as_str() != RDF_TYPE {
            continue;
        }
        let Some(iri) = term_iri(&triple.object) else {
            continue;
        };
        let ranked = match iri.as_str() {
            OWL_DATATYPE => Some((4, PropertyType::DatatypeProperty)),
            OWL_OBJECT => Some((3, PropertyType::ObjectProperty)),
            OWL_ANNOTATION => Some((2, PropertyType::AnnotationProperty)),
            RDF_PROPERTY => Some((1, PropertyType::ObjectProperty)),
            _ => None,
        };
        if let Some(ranked) = ranked
            && best.is_none_or(|(rank, _)| ranked.0 > rank)
        {
            best = Some(ranked);
        }
    }
    best.map(|(_, kind)| kind)
}

fn typed_as(triples: &[Triple], type_iri: &str) -> bool {
    triples.iter().any(|triple| {
        triple.predicate.as_str() == RDF_TYPE
            && term_iri(&triple.object).as_deref() == Some(type_iri)
    })
}

fn term_iri(term: &Term) -> Option<String> {
    match term {
        Term::NamedNode(node) => Some(node.as_str().to_string()),
        _ => None,
    }
}

/// Prefer a language-less literal, then English, then the first literal.
fn preferred_literal(terms: &[&Term]) -> String {
    let literals: Vec<&Literal> = terms
        .iter()
        .filter_map(|term| match term {
            Term::Literal(literal) => Some(literal),
            _ => None,
        })
        .collect();
    let text = |literal: &Literal| literal.value().to_string();
    if let Some(literal) = literals.iter().find(|literal| literal.language().is_none()) {
        return text(literal);
    }
    if let Some(literal) = literals.iter().find(|literal| {
        literal
            .language()
            .is_some_and(|tag| tag == "en" || tag.starts_with("en-"))
    }) {
        return text(literal);
    }
    literals
        .first()
        .map(|literal| text(literal))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(ttl: &str) -> Ontology {
        parse_turtle(ttl.as_bytes(), "http://example.org/test").unwrap()
    }

    #[test]
    fn parses_classes_hierarchy_and_properties() {
        let ttl = include_str!("../testdata/simple.ttl");
        let ontology = parse(ttl);
        let person = ontology
            .classes
            .get("http://example.org/test#Person")
            .unwrap();
        assert_eq!(person.label, "Person");
        assert_eq!(person.comment, "A human being");
        assert_eq!(
            person.parents,
            vec!["http://example.org/test#Agent".to_string()]
        );
        let agent = ontology
            .classes
            .get("http://example.org/test#Agent")
            .unwrap();
        assert_eq!(
            agent.children,
            vec!["http://example.org/test#Person".to_string()]
        );

        let name = ontology
            .properties
            .get("http://example.org/test#name")
            .unwrap();
        assert_eq!(name.kind, PropertyType::DatatypeProperty);
        assert_eq!(
            name.domain,
            vec!["http://example.org/test#Agent".to_string()]
        );
        assert!(ontology.namespaces.contains_key("ex"));
    }

    #[test]
    fn reads_schema_domain_includes_and_rdf_property() {
        let ttl = include_str!("../testdata/schema.ttl");
        let ontology = parse(ttl);
        let birth = ontology
            .properties
            .get("https://schema.org/birthPlace")
            .unwrap();
        assert_eq!(birth.kind, PropertyType::ObjectProperty);
        assert_eq!(birth.domain, vec!["https://schema.org/Person".to_string()]);
        assert_eq!(birth.range, vec!["https://schema.org/Place".to_string()]);
    }

    #[test]
    fn reads_owl_restriction_cardinality() {
        let ttl = include_str!("../testdata/restrictions.ttl");
        let ontology = parse(ttl);
        let document = ontology
            .classes
            .get("http://example.org/r#Document")
            .unwrap();
        assert_eq!(document.properties.len(), 1);
        assert_eq!(document.properties[0].uri, "http://example.org/r#title");
        assert!(document.properties[0].required);
        assert_eq!(document.properties[0].cardinality, "1..*");
        assert!(document.deprecated);
    }

    #[test]
    fn reads_imports() {
        let ttl = r#"
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            <http://example.org/test> a owl:Ontology ;
                owl:imports <http://example.org/other> .
        "#;
        let ontology = parse(ttl);
        assert_eq!(
            ontology.imports,
            vec!["http://example.org/other".to_string()]
        );
    }

    #[test]
    fn rejects_broken_turtle() {
        let err = parse_turtle(&b"this is not turtle"[..], "http://example.org/").unwrap_err();
        assert!(matches!(err, Error::Parse(_)));
    }
}
