use std::collections::BTreeMap;

use serde::Serialize;

/// Whether a property points at a resource, a literal, or is only an annotation.
///
/// Serialized in snake case (`"object_property"` and so on). `rdf:Property`,
/// which Schema.org uses, is stored as [`PropertyType::ObjectProperty`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyType {
    ObjectProperty,
    DatatypeProperty,
    AnnotationProperty,
}

impl PropertyType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ObjectProperty => "object_property",
            Self::DatatypeProperty => "datatype_property",
            Self::AnnotationProperty => "annotation_property",
        }
    }
}

/// One parsed ontology. Maps are keyed by full IRI and ordered so repeated
/// queries print classes and properties in a stable order.
#[derive(Debug, Clone)]
pub struct Ontology {
    pub uri: String,
    pub imports: Vec<String>,
    pub classes: BTreeMap<String, Class>,
    pub properties: BTreeMap<String, Property>,
    pub namespaces: BTreeMap<String, String>,
    /// Typed things in the file, such as a news article that uses the vocabulary.
    pub resources: Vec<Resource>,
}

/// One typed thing in a document. A schema.org news article is one of these,
/// not a class declaration.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Resource {
    pub id: String,
    pub types: Vec<String>,
    pub values: Vec<ResourceValue>,
}

/// One predicate on a [`Resource`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResourceValue {
    pub predicate: String,
    pub value: String,
    pub kind: TermKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TermKind {
    Iri,
    Blank,
    Literal,
}

/// A named class.
#[derive(Debug, Clone, Serialize)]
pub struct Class {
    pub uri: String,
    pub ontology_uri: String,
    pub label: String,
    pub comment: String,
    pub description: String,
    pub parents: Vec<String>,
    pub children: Vec<String>,
    pub properties: Vec<PropertyRef>,
    pub deprecated: bool,
}

/// A named property.
#[derive(Debug, Clone, Serialize)]
pub struct Property {
    pub uri: String,
    pub ontology_uri: String,
    pub label: String,
    pub comment: String,
    pub description: String,
    pub domain: Vec<String>,
    pub range: Vec<String>,
    #[serde(rename = "type")]
    pub kind: PropertyType,
}

/// A property attached to a class through an OWL restriction.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyRef {
    pub uri: String,
    pub required: bool,
    pub cardinality: String,
    pub source_class: String,
}

impl Ontology {
    pub fn new(uri: impl Into<String>) -> Self {
        let mut namespaces = BTreeMap::new();
        // Always available, even when the file never declares them. A file
        // prefix with the same name overwrites these during parsing.
        for (prefix, iri) in [
            ("owl", "http://www.w3.org/2002/07/owl#"),
            ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
            ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
            ("xsd", "http://www.w3.org/2001/XMLSchema#"),
            ("dcterms", "http://purl.org/dc/terms/"),
            ("schema", "https://schema.org/"),
        ] {
            namespaces.insert(prefix.to_string(), iri.to_string());
        }
        Self {
            uri: uri.into(),
            imports: Vec::new(),
            classes: BTreeMap::new(),
            properties: BTreeMap::new(),
            namespaces,
            resources: Vec::new(),
        }
    }

    /// Expand `prefix:Name` with a declared namespace. A full IRI, or a name
    /// whose prefix is unknown, is returned unchanged.
    pub fn resolve_prefix(&self, name: &str) -> String {
        if name.starts_with("http://") || name.starts_with("https://") {
            return name.to_string();
        }
        let Some((prefix, local)) = name.split_once(':') else {
            return name.to_string();
        };
        match self.namespaces.get(prefix) {
            Some(ns) => format!("{ns}{local}"),
            None => name.to_string(),
        }
    }

    /// Abbreviate an IRI with the longest matching namespace, if one exists.
    pub fn get_prefix(&self, uri: &str) -> String {
        let mut best: Option<(&str, &str)> = None;
        for (prefix, ns) in &self.namespaces {
            if uri.starts_with(ns.as_str())
                && best.is_none_or(|(_, existing)| ns.len() > existing.len())
            {
                best = Some((prefix.as_str(), ns.as_str()));
            }
        }
        match best {
            Some((prefix, ns)) => format!("{prefix}:{}", &uri[ns.len()..]),
            None => uri.to_string(),
        }
    }

    /// Fold another ontology into this one. Terms already declared here keep
    /// their labels. Parents, restriction properties, domains, and ranges are
    /// united, then subclass links are rebuilt across both files.
    pub fn absorb(&mut self, other: &Ontology) {
        for (prefix, iri) in &other.namespaces {
            self.namespaces
                .entry(prefix.clone())
                .or_insert_with(|| iri.clone());
        }
        for iri in &other.imports {
            push_unique(&mut self.imports, iri.clone());
        }
        for (uri, class) in &other.classes {
            if let Some(existing) = self.classes.get_mut(uri) {
                for parent in &class.parents {
                    push_unique(&mut existing.parents, parent.clone());
                }
                for property in &class.properties {
                    if !existing
                        .properties
                        .iter()
                        .any(|item| item.uri == property.uri)
                    {
                        existing.properties.push(property.clone());
                    }
                }
                fill_empty(&mut existing.label, &class.label);
                fill_empty(&mut existing.comment, &class.comment);
                fill_empty(&mut existing.description, &class.description);
            } else {
                self.classes.insert(uri.clone(), class.clone());
            }
        }
        for (uri, property) in &other.properties {
            if let Some(existing) = self.properties.get_mut(uri) {
                for domain in &property.domain {
                    push_unique(&mut existing.domain, domain.clone());
                }
                for range in &property.range {
                    push_unique(&mut existing.range, range.clone());
                }
                fill_empty(&mut existing.label, &property.label);
                fill_empty(&mut existing.comment, &property.comment);
                fill_empty(&mut existing.description, &property.description);
            } else {
                self.properties.insert(uri.clone(), property.clone());
            }
        }
        for resource in &other.resources {
            if !self.resources.iter().any(|item| item.id == resource.id) {
                self.resources.push(resource.clone());
            }
        }
        self.relink_children();
    }

    fn relink_children(&mut self) {
        for class in self.classes.values_mut() {
            class.children.clear();
        }
        let edges: Vec<(String, String)> = self
            .classes
            .iter()
            .flat_map(|(uri, class)| {
                class
                    .parents
                    .iter()
                    .cloned()
                    .map(move |parent| (uri.clone(), parent))
            })
            .collect();
        for (child, parent) in edges {
            if let Some(parent_class) = self.classes.get_mut(&parent) {
                push_unique(&mut parent_class.children, child);
            }
        }
    }
}

fn fill_empty(slot: &mut String, value: &str) {
    if slot.is_empty() && !value.is_empty() {
        *slot = value.to_string();
    }
}

pub(crate) fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}
