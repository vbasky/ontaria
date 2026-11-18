//! The indexed ontology.
//!
//! An [`Ontology`] is immutable after [`Ontology::load`](crate::Ontology::load)
//! returns. It is `Send + Sync`, so share it as `Arc<Ontology>`. Query methods
//! take `&self` and do not touch the network or the disk.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Warning;

/// How to choose one string from several language-tagged values.
///
/// An empty preference keeps the historical rule: a language-less value wins,
/// then English, then the first value. A non-empty list is tried in order.
/// A tag matches itself and tags that start with `{tag}-` (`en` matches `en-GB`).
/// A language-less value is the fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LangPref {
    pub tags: Vec<String>,
}

impl LangPref {
    pub fn new(tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            tags: tags.into_iter().map(Into::into).collect(),
        }
    }

    /// Pick the preferred text. Returns an empty string when `values` is empty.
    pub fn prefer(&self, values: &[LangString]) -> String {
        if values.is_empty() {
            return String::new();
        }
        if self.tags.is_empty() {
            if let Some(plain) = values.iter().find(|item| item.language.is_none()) {
                return plain.value.clone();
            }
            if let Some(english) = values
                .iter()
                .find(|item| is_english(item.language.as_deref()))
            {
                return english.value.clone();
            }
            return values[0].value.clone();
        }
        for tag in &self.tags {
            if let Some(found) = values
                .iter()
                .find(|item| item.language.as_deref() == Some(tag.as_str()))
            {
                return found.value.clone();
            }
            let prefix = format!("{tag}-");
            if let Some(found) = values.iter().find(|item| {
                item.language
                    .as_deref()
                    .is_some_and(|lang| lang.starts_with(&prefix))
            }) {
                return found.value.clone();
            }
        }
        if let Some(plain) = values.iter().find(|item| item.language.is_none()) {
            return plain.value.clone();
        }
        values[0].value.clone()
    }
}

fn is_english(tag: Option<&str>) -> bool {
    tag.is_some_and(|tag| tag == "en" || tag.starts_with("en-"))
}

/// One literal and the language tag it was written with, when it had one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LangString {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl LangString {
    pub fn plain(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            language: None,
        }
    }

    pub fn tagged(value: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            language: Some(language.into()),
        }
    }
}

/// Whether a property points at a resource, a literal, or is only an annotation.
///
/// Serialized in snake case (`"object_property"` and so on). `rdf:Property`,
/// which Schema.org uses, is stored as [`PropertyType::ObjectProperty`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// A class, property, or datatype expression used by OWL constructors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClassExpression {
    Class { iri: String },
    Union { operands: Vec<ClassExpression> },
    Intersection { operands: Vec<ClassExpression> },
    Complement { operand: Box<ClassExpression> },
    OneOf { individuals: Vec<String> },
}

/// An annotation that is not already a dedicated field on the term.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Annotation {
    pub property: String,
    pub value: String,
    pub kind: TermKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// One triple kept from the parsed document.
///
/// `inferred` is false for triples that were in the file. Reasoning sets it
/// on triples that were entailed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Statement {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub kind: TermKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inferred: bool,
}

/// A value that came from reasoning, or that was written in the file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fact<T> {
    pub value: T,
    /// False when a reasoner added this value. Asserted values are true.
    pub asserted: bool,
}

/// One parsed ontology.
///
/// Maps are keyed by full IRI and ordered so repeated queries print terms in
/// a stable order. Build one with [`crate::parse_document`] or
/// [`Ontology::load`], then share it. Query methods do not fetch or parse.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Ontology {
    pub uri: String,
    /// Syntax the bytes were read as, such as `"turtle"` or `"json-ld"`.
    pub format: String,
    pub imports: Vec<String>,
    pub metadata: OntologyMetadata,
    pub classes: BTreeMap<String, Class>,
    pub properties: BTreeMap<String, Property>,
    pub individuals: BTreeMap<String, Individual>,
    pub concepts: BTreeMap<String, Concept>,
    pub schemes: BTreeMap<String, ConceptScheme>,
    /// Prefixes available for resolution, including the built-in ones.
    pub namespaces: BTreeMap<String, String>,
    /// Prefixes that appeared in the file, not the ones ontaria inserts.
    pub declared_prefixes: BTreeMap<String, String>,
    /// Typed things in the file, such as a news article that uses the vocabulary.
    pub resources: Vec<Resource>,
    /// Every triple the index was built from.
    pub statements: Vec<Statement>,
    pub warnings: Vec<Warning>,
}

/// `owl:Ontology` header fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OntologyMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_iri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_info: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub see_also: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
}

/// One typed thing in a document. A schema.org news article is one of these,
/// not a class declaration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Resource {
    pub id: String,
    pub types: Vec<String>,
    pub values: Vec<ResourceValue>,
}

/// One predicate on a [`Resource`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceValue {
    pub predicate: String,
    pub value: String,
    pub kind: TermKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TermKind {
    Iri,
    Blank,
    Literal,
}

/// A named class.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Class {
    pub uri: String,
    pub ontology_uri: String,
    pub label: String,
    pub comment: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<LangString>,
    pub parents: Vec<String>,
    pub children: Vec<String>,
    pub properties: Vec<PropertyRef>,
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub see_also: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub is_defined_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equivalent_classes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equivalent_expressions: Vec<ClassExpression>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disjoint_with: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub union_of: Vec<ClassExpression>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intersection_of: Vec<ClassExpression>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complement_of: Option<ClassExpression>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub one_of: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
}

/// A named property.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Property {
    pub uri: String,
    pub ontology_uri: String,
    pub label: String,
    pub comment: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub descriptions: Vec<LangString>,
    pub domain: Vec<String>,
    pub range: Vec<String>,
    #[serde(rename = "type")]
    pub kind: PropertyType,
    #[serde(default, skip_serializing_if = "is_false")]
    pub deprecated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub super_properties: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_properties: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equivalent_properties: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inverse_of: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub functional: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inverse_functional: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub transitive: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub symmetric: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub see_also: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub is_defined_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domain_expressions: Vec<ClassExpression>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub range_expressions: Vec<ClassExpression>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
}

/// A property attached to a class through an OWL restriction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropertyRef {
    pub uri: String,
    pub required: bool,
    pub cardinality: String,
    pub source_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub some_values_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_values_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact: Option<u64>,
}

/// `owl:NamedIndividual`, or a subject typed as one of the ontology's classes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Individual {
    pub uri: String,
    pub types: Vec<String>,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
}

/// A `skos:Concept`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Concept {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schemes: Vec<String>,
    pub pref_label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pref_labels: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alt_labels: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden_labels: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub broader: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub narrower: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exact_match: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub close_match: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
}

/// A `skos:ConceptScheme`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConceptScheme {
    pub uri: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LangString>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_concepts: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Class {
    pub(crate) fn named(uri: impl Into<String>, ontology_uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            ontology_uri: ontology_uri.into(),
            label: String::new(),
            comment: String::new(),
            description: String::new(),
            labels: Vec::new(),
            comments: Vec::new(),
            descriptions: Vec::new(),
            parents: Vec::new(),
            children: Vec::new(),
            properties: Vec::new(),
            deprecated: false,
            see_also: Vec::new(),
            is_defined_by: Vec::new(),
            equivalent_classes: Vec::new(),
            equivalent_expressions: Vec::new(),
            disjoint_with: Vec::new(),
            union_of: Vec::new(),
            intersection_of: Vec::new(),
            complement_of: None,
            one_of: Vec::new(),
            annotations: Vec::new(),
        }
    }
}

impl Property {
    pub(crate) fn named(
        uri: impl Into<String>,
        ontology_uri: impl Into<String>,
        kind: PropertyType,
    ) -> Self {
        Self {
            uri: uri.into(),
            ontology_uri: ontology_uri.into(),
            label: String::new(),
            comment: String::new(),
            description: String::new(),
            labels: Vec::new(),
            comments: Vec::new(),
            descriptions: Vec::new(),
            domain: Vec::new(),
            range: Vec::new(),
            kind,
            deprecated: false,
            super_properties: Vec::new(),
            sub_properties: Vec::new(),
            equivalent_properties: Vec::new(),
            inverse_of: Vec::new(),
            functional: false,
            inverse_functional: false,
            transitive: false,
            symmetric: false,
            see_also: Vec::new(),
            is_defined_by: Vec::new(),
            domain_expressions: Vec::new(),
            range_expressions: Vec::new(),
            annotations: Vec::new(),
        }
    }
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
            ("skos", "http://www.w3.org/2004/02/skos/core#"),
            ("sh", "http://www.w3.org/ns/shacl#"),
            ("prov", "http://www.w3.org/ns/prov#"),
            ("schema", "https://schema.org/"),
        ] {
            namespaces.insert(prefix.to_string(), iri.to_string());
        }
        Self {
            uri: uri.into(),
            format: String::new(),
            imports: Vec::new(),
            metadata: OntologyMetadata::default(),
            classes: BTreeMap::new(),
            properties: BTreeMap::new(),
            individuals: BTreeMap::new(),
            concepts: BTreeMap::new(),
            schemes: BTreeMap::new(),
            namespaces,
            declared_prefixes: BTreeMap::new(),
            resources: Vec::new(),
            statements: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Return a copy whose display labels follow `pref`.
    ///
    /// The language-tagged values themselves are unchanged. Only the single
    /// `label`, `comment`, `description`, and `pref_label` strings move.
    pub fn with_language(&self, pref: &LangPref) -> Self {
        let mut copy = self.clone();
        copy.apply_language(pref);
        copy
    }

    pub(crate) fn apply_language(&mut self, pref: &LangPref) {
        for class in self.classes.values_mut() {
            if !class.labels.is_empty() {
                class.label = pref.prefer(&class.labels);
            }
            if !class.comments.is_empty() {
                class.comment = pref.prefer(&class.comments);
            }
            if !class.descriptions.is_empty() {
                class.description = pref.prefer(&class.descriptions);
            }
        }
        for property in self.properties.values_mut() {
            if !property.labels.is_empty() {
                property.label = pref.prefer(&property.labels);
            }
            if !property.comments.is_empty() {
                property.comment = pref.prefer(&property.comments);
            }
            if !property.descriptions.is_empty() {
                property.description = pref.prefer(&property.descriptions);
            }
        }
        for individual in self.individuals.values_mut() {
            if !individual.labels.is_empty() {
                individual.label = pref.prefer(&individual.labels);
            }
        }
        for concept in self.concepts.values_mut() {
            if !concept.pref_labels.is_empty() {
                concept.pref_label = pref.prefer(&concept.pref_labels);
            }
        }
        for scheme in self.schemes.values_mut() {
            if !scheme.labels.is_empty() {
                scheme.label = pref.prefer(&scheme.labels);
            }
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
        for (prefix, iri) in &other.declared_prefixes {
            self.declared_prefixes
                .entry(prefix.clone())
                .or_insert_with(|| iri.clone());
        }
        for iri in &other.imports {
            push_unique(&mut self.imports, iri.clone());
        }
        merge_metadata(&mut self.metadata, &other.metadata);
        for (uri, class) in &other.classes {
            if let Some(existing) = self.classes.get_mut(uri) {
                merge_class(existing, class);
            } else {
                self.classes.insert(uri.clone(), class.clone());
            }
        }
        for (uri, property) in &other.properties {
            if let Some(existing) = self.properties.get_mut(uri) {
                merge_property(existing, property);
            } else {
                self.properties.insert(uri.clone(), property.clone());
            }
        }
        for (uri, individual) in &other.individuals {
            self.individuals
                .entry(uri.clone())
                .or_insert_with(|| individual.clone());
        }
        for (uri, concept) in &other.concepts {
            self.concepts
                .entry(uri.clone())
                .or_insert_with(|| concept.clone());
        }
        for (uri, scheme) in &other.schemes {
            self.schemes
                .entry(uri.clone())
                .or_insert_with(|| scheme.clone());
        }
        for resource in &other.resources {
            if !self.resources.iter().any(|item| item.id == resource.id) {
                self.resources.push(resource.clone());
            }
        }
        for statement in &other.statements {
            if !self
                .statements
                .iter()
                .any(|item| same_statement(item, statement))
            {
                self.statements.push(statement.clone());
            }
        }
        for warning in &other.warnings {
            if !self.warnings.iter().any(|item| item == warning) {
                self.warnings.push(warning.clone());
            }
        }
        self.relink_children();
        self.relink_subproperties();
    }

    pub(crate) fn relink_children(&mut self) {
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

    pub(crate) fn relink_subproperties(&mut self) {
        for property in self.properties.values_mut() {
            property.sub_properties.clear();
        }
        let edges: Vec<(String, String)> = self
            .properties
            .iter()
            .flat_map(|(uri, property)| {
                property
                    .super_properties
                    .iter()
                    .cloned()
                    .map(move |parent| (uri.clone(), parent))
            })
            .collect();
        for (child, parent) in edges {
            if let Some(parent_property) = self.properties.get_mut(&parent) {
                push_unique(&mut parent_property.sub_properties, child);
            }
        }
    }
}

fn same_statement(left: &Statement, right: &Statement) -> bool {
    left.subject == right.subject
        && left.predicate == right.predicate
        && left.object == right.object
        && left.kind == right.kind
        && left.language == right.language
        && left.graph == right.graph
        && left.inferred == right.inferred
}

fn merge_metadata(into: &mut OntologyMetadata, other: &OntologyMetadata) {
    if into.version_iri.is_none() {
        into.version_iri.clone_from(&other.version_iri);
    }
    if into.version_info.is_none() {
        into.version_info.clone_from(&other.version_info);
    }
    if into.prior_version.is_none() {
        into.prior_version.clone_from(&other.prior_version);
    }
    if into.created.is_none() {
        into.created.clone_from(&other.created);
    }
    if into.modified.is_none() {
        into.modified.clone_from(&other.modified);
    }
    if into.license.is_none() {
        into.license.clone_from(&other.license);
    }
    merge_lang(&mut into.labels, &other.labels);
    merge_lang(&mut into.comments, &other.comments);
    merge_strings(&mut into.see_also, &other.see_also);
    merge_annotations(&mut into.annotations, &other.annotations);
}

fn merge_class(existing: &mut Class, class: &Class) {
    for parent in &class.parents {
        push_unique(&mut existing.parents, parent.clone());
    }
    for property in &class.properties {
        if let Some(slot) = existing
            .properties
            .iter_mut()
            .find(|item| item.uri == property.uri)
        {
            merge_property_ref(slot, property);
        } else {
            existing.properties.push(property.clone());
        }
    }
    fill_empty(&mut existing.label, &class.label);
    fill_empty(&mut existing.comment, &class.comment);
    fill_empty(&mut existing.description, &class.description);
    existing.deprecated |= class.deprecated;
    merge_lang(&mut existing.labels, &class.labels);
    merge_lang(&mut existing.comments, &class.comments);
    merge_lang(&mut existing.descriptions, &class.descriptions);
    merge_strings(&mut existing.see_also, &class.see_also);
    merge_strings(&mut existing.is_defined_by, &class.is_defined_by);
    merge_strings(&mut existing.equivalent_classes, &class.equivalent_classes);
    merge_expr(
        &mut existing.equivalent_expressions,
        &class.equivalent_expressions,
    );
    merge_strings(&mut existing.disjoint_with, &class.disjoint_with);
    merge_expr(&mut existing.union_of, &class.union_of);
    merge_expr(&mut existing.intersection_of, &class.intersection_of);
    if existing.complement_of.is_none() {
        existing.complement_of.clone_from(&class.complement_of);
    }
    merge_strings(&mut existing.one_of, &class.one_of);
    merge_annotations(&mut existing.annotations, &class.annotations);
}

fn merge_property(existing: &mut Property, property: &Property) {
    for domain in &property.domain {
        push_unique(&mut existing.domain, domain.clone());
    }
    for range in &property.range {
        push_unique(&mut existing.range, range.clone());
    }
    fill_empty(&mut existing.label, &property.label);
    fill_empty(&mut existing.comment, &property.comment);
    fill_empty(&mut existing.description, &property.description);
    existing.deprecated |= property.deprecated;
    existing.functional |= property.functional;
    existing.inverse_functional |= property.inverse_functional;
    existing.transitive |= property.transitive;
    existing.symmetric |= property.symmetric;
    if existing.kind == PropertyType::ObjectProperty
        && property.kind != PropertyType::ObjectProperty
    {
        existing.kind = property.kind;
    }
    merge_strings(&mut existing.super_properties, &property.super_properties);
    merge_strings(
        &mut existing.equivalent_properties,
        &property.equivalent_properties,
    );
    merge_strings(&mut existing.inverse_of, &property.inverse_of);
    merge_strings(&mut existing.see_also, &property.see_also);
    merge_strings(&mut existing.is_defined_by, &property.is_defined_by);
    merge_lang(&mut existing.labels, &property.labels);
    merge_lang(&mut existing.comments, &property.comments);
    merge_lang(&mut existing.descriptions, &property.descriptions);
    merge_expr(
        &mut existing.domain_expressions,
        &property.domain_expressions,
    );
    merge_expr(&mut existing.range_expressions, &property.range_expressions);
    merge_annotations(&mut existing.annotations, &property.annotations);
}

fn merge_property_ref(slot: &mut PropertyRef, property: &PropertyRef) {
    slot.required |= property.required;
    if slot.cardinality.is_empty() {
        slot.cardinality.clone_from(&property.cardinality);
    }
    if slot.on_class.is_none() {
        slot.on_class.clone_from(&property.on_class);
    }
    if slot.some_values_from.is_none() {
        slot.some_values_from.clone_from(&property.some_values_from);
    }
    if slot.all_values_from.is_none() {
        slot.all_values_from.clone_from(&property.all_values_from);
    }
    if slot.has_value.is_none() {
        slot.has_value.clone_from(&property.has_value);
    }
    if slot.min.is_none() {
        slot.min = property.min;
    }
    if slot.max.is_none() {
        slot.max = property.max;
    }
    if slot.exact.is_none() {
        slot.exact = property.exact;
    }
}

fn merge_strings(into: &mut Vec<String>, extra: &[String]) {
    for value in extra {
        push_unique(into, value.clone());
    }
}

fn merge_lang(into: &mut Vec<LangString>, extra: &[LangString]) {
    for value in extra {
        if !into
            .iter()
            .any(|item| item.value == value.value && item.language == value.language)
        {
            into.push(value.clone());
        }
    }
}

fn merge_expr(into: &mut Vec<ClassExpression>, extra: &[ClassExpression]) {
    for value in extra {
        if !into.iter().any(|item| item == value) {
            into.push(value.clone());
        }
    }
}

fn merge_annotations(into: &mut Vec<Annotation>, extra: &[Annotation]) {
    for value in extra {
        if !into.iter().any(|item| {
            item.property == value.property
                && item.value == value.value
                && item.language == value.language
        }) {
            into.push(value.clone());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ontology_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Ontology>();
    }

    #[test]
    fn language_preference_picks_the_requested_tag() {
        let values = vec![
            LangString::plain("plain"),
            LangString::tagged("english", "en"),
            LangString::tagged("francais", "fr"),
        ];
        assert_eq!(LangPref::default().prefer(&values), "plain");
        assert_eq!(LangPref::new(["fr"]).prefer(&values), "francais");
        assert_eq!(LangPref::new(["de", "en"]).prefer(&values), "english");
    }
}
