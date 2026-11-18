//! Check instance data against a loaded ontology.
//!
//! The checker accepts a JSON document or an [`oxrdf::Graph`]. It does not
//! read files. Unknown terms, domain and range, cardinality, enumerations,
//! disjoint classes, and deprecated terms each produce a [`ValidationIssue`].

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::{Graph, NamedOrBlankNodeRef, TermRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;
use crate::model::{Class, ClassExpression, Ontology, Property, PropertyRef, PropertyType};
use crate::vocab::{self, local_name};

/// How serious a validation finding is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}

/// One finding. `path` is a JSON pointer-like location such as `$.name`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub term: String,
    pub rule: String,
    pub severity: Severity,
    pub message: String,
}

/// The full set of findings, in a stable order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .count()
    }

    /// A W3C SHACL Validation Report in Turtle.
    ///
    /// Ontaria's own rules are reported as `sh:ValidationResult` nodes. This
    /// is the same shape a SHACL engine emits, so CI can read either source.
    pub fn to_turtle(&self) -> String {
        let mut lines = vec![
            "@prefix sh: <http://www.w3.org/ns/shacl#> .".to_string(),
            "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .".to_string(),
            "[] a sh:ValidationReport ;".to_string(),
            format!(
                "  sh:conforms {} ;",
                if self.has_errors() { "false" } else { "true" }
            ),
        ];
        for (index, issue) in self.issues.iter().enumerate() {
            let comma = if index + 1 == self.issues.len() {
                " ."
            } else {
                " ;"
            };
            let severity = match issue.severity {
                Severity::Error => "sh:Violation",
                Severity::Warning => "sh:Warning",
            };
            lines.push("  sh:result [".to_string());
            lines.push("    a sh:ValidationResult ;".to_string());
            lines.push(format!("    sh:resultSeverity {severity} ;"));
            lines.push(format!("    sh:focusNode \"{}\" ;", escape(&issue.path)));
            lines.push(format!(
                "    sh:sourceConstraintComponent <{}> ;",
                rule_iri(&issue.rule)
            ));
            if !issue.term.is_empty() {
                if issue.term.starts_with("http://") || issue.term.starts_with("https://") {
                    lines.push(format!("    sh:resultPath <{}> ;", issue.term));
                } else {
                    lines.push(format!("    sh:value \"{}\" ;", escape(&issue.term)));
                }
            }
            lines.push(format!(
                "    sh:resultMessage \"{}\"",
                escape(&issue.message)
            ));
            lines.push(format!("  ]{comma}"));
        }
        if self.issues.is_empty() {
            lines.pop();
            lines.push("  sh:conforms true .".to_string());
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

/// Validate a JSON document. Objects may use `@type`/`type` and property keys
/// that are IRIs, `prefix:Name`, or local names.
pub fn validate_json(ontology: &Ontology, value: &Value) -> ValidationReport {
    let mut instances = Vec::new();
    collect_json(ontology, value, "$", &mut instances);
    check_instances(ontology, &instances)
}

/// Validate triples already parsed into an [`Graph`].
pub fn validate_graph(ontology: &Ontology, graph: &Graph) -> ValidationReport {
    let instances = instances_from_graph(ontology, graph);
    check_instances(ontology, &instances)
}

/// Validate by reading Turtle, N-Triples, or another syntax ontaria already parses.
pub fn validate_rdf(ontology: &Ontology, bytes: &[u8], base_uri: &str) -> Result<ValidationReport> {
    let parsed = crate::parser::parse_document(bytes, base_uri)?;
    Ok(validate_statements(ontology, &parsed))
}

pub fn validate_statements(ontology: &Ontology, data: &Ontology) -> ValidationReport {
    let mut graph = Graph::new();
    for statement in &data.statements {
        if let Some(triple) = crate::extract::statement_to_triple(statement) {
            graph.insert(&triple);
        }
    }
    validate_graph(ontology, &graph)
}

#[derive(Debug, Clone)]
struct Instance {
    id: String,
    path: String,
    types: Vec<String>,
    values: BTreeMap<String, Vec<NodeValue>>,
}

#[derive(Debug, Clone)]
enum NodeValue {
    Iri(String),
    Literal {
        value: String,
        datatype: Option<String>,
    },
}

fn collect_json(ontology: &Ontology, value: &Value, path: &str, into: &mut Vec<Instance>) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_json(ontology, item, &format!("{path}[{index}]"), into);
            }
        }
        Value::Object(map) => {
            let id = json_string(map.get("@id").or_else(|| map.get("id")))
                .unwrap_or_else(|| format!("_:{}", path.trim_start_matches('$')));
            let types = json_types(ontology, map.get("@type").or_else(|| map.get("type")));
            let mut values = BTreeMap::new();
            for (key, child) in map {
                if matches!(key.as_str(), "@id" | "id" | "@type" | "type" | "@context") {
                    continue;
                }
                let iri = resolve_term(ontology, key);
                let nodes = json_values(ontology, child);
                if !nodes.is_empty() {
                    values.insert(iri, nodes);
                }
                if let Value::Object(_) | Value::Array(_) = child {
                    collect_json(ontology, child, &format!("{path}.{key}"), into);
                }
            }
            into.push(Instance {
                id,
                path: path.to_string(),
                types,
                values,
            });
        }
        _ => {}
    }
}

fn json_values(ontology: &Ontology, value: &Value) -> Vec<NodeValue> {
    match value {
        Value::Array(items) => items
            .iter()
            .flat_map(|item| json_values(ontology, item))
            .collect(),
        Value::String(text) => vec![NodeValue::Literal {
            value: text.clone(),
            datatype: None,
        }],
        Value::Number(number) => vec![NodeValue::Literal {
            value: number.to_string(),
            datatype: Some(format!("{}integer", vocab::XSD)),
        }],
        Value::Bool(flag) => vec![NodeValue::Literal {
            value: flag.to_string(),
            datatype: Some(format!("{}boolean", vocab::XSD)),
        }],
        Value::Object(map) => {
            if let Some(id) = json_string(map.get("@id").or_else(|| map.get("id"))) {
                vec![NodeValue::Iri(resolve_term(ontology, &id))]
            } else {
                Vec::new()
            }
        }
        Value::Null => Vec::new(),
    }
}

fn json_types(ontology: &Ontology, value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(text)) => vec![resolve_term(ontology, text)],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(|text| resolve_term(ontology, text)))
            .collect(),
        _ => Vec::new(),
    }
}

fn json_string(value: Option<&Value>) -> Option<String> {
    value.and_then(|item| item.as_str()).map(str::to_string)
}

fn instances_from_graph(ontology: &Ontology, graph: &Graph) -> Vec<Instance> {
    let _ = ontology;
    let mut types: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut values: BTreeMap<String, BTreeMap<String, Vec<NodeValue>>> = BTreeMap::new();
    for triple in graph.iter() {
        let subject = match triple.subject {
            NamedOrBlankNodeRef::NamedNode(node) => node.as_str().to_string(),
            NamedOrBlankNodeRef::BlankNode(node) => format!("_:{}", node.as_str()),
        };
        if triple.predicate.as_str() == vocab::RDF_TYPE {
            if let TermRef::NamedNode(node) = triple.object {
                types
                    .entry(subject)
                    .or_default()
                    .push(node.as_str().to_string());
            }
            continue;
        }
        let predicate = triple.predicate.as_str().to_string();
        let node = match triple.object {
            TermRef::NamedNode(node) => NodeValue::Iri(node.as_str().to_string()),
            TermRef::BlankNode(node) => NodeValue::Iri(format!("_:{}", node.as_str())),
            TermRef::Literal(literal) => NodeValue::Literal {
                value: literal.value().to_string(),
                datatype: Some(literal.datatype().as_str().to_string()),
            },
            #[allow(unreachable_patterns)]
            _ => continue,
        };
        values
            .entry(subject)
            .or_default()
            .entry(predicate)
            .or_default()
            .push(node);
    }
    let mut ids: BTreeSet<String> = types.keys().cloned().collect();
    ids.extend(values.keys().cloned());
    ids.into_iter()
        .map(|id| Instance {
            path: id.clone(),
            types: types.remove(&id).unwrap_or_default(),
            values: values.remove(&id).unwrap_or_default(),
            id,
        })
        .collect()
}

fn check_instances(ontology: &Ontology, instances: &[Instance]) -> ValidationReport {
    let mut issues = Vec::new();
    for instance in instances {
        check_instance(ontology, instance, &mut issues);
    }
    issues.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.rule.cmp(&right.rule))
            .then(left.term.cmp(&right.term))
            .then(left.message.cmp(&right.message))
    });
    issues.dedup();
    ValidationReport { issues }
}

fn check_instance(ontology: &Ontology, instance: &Instance, issues: &mut Vec<ValidationIssue>) {
    let mut classes = Vec::new();
    for type_iri in &instance.types {
        if vocab::is_builtin_namespace(type_iri) {
            continue;
        }
        match ontology.classes.get(type_iri) {
            Some(class) => {
                if class.deprecated {
                    issues.push(issue(
                        &instance.path,
                        type_iri,
                        "deprecated",
                        Severity::Warning,
                        format!("type {type_iri} is deprecated"),
                    ));
                }
                if !class.one_of.is_empty() && !class.one_of.iter().any(|iri| iri == &instance.id) {
                    issues.push(issue(
                        &instance.path,
                        type_iri,
                        "one_of",
                        Severity::Error,
                        format!(
                            "{id} is not one of the allowed values of {type_iri}",
                            id = instance.id
                        ),
                    ));
                }
                classes.push(class);
            }
            None if ontology.concepts.contains_key(type_iri) => {}
            None => issues.push(issue(
                &instance.path,
                type_iri,
                "unknown_type",
                Severity::Error,
                format!("unknown type {type_iri}"),
            )),
        }
    }
    check_disjoint(ontology, instance, issues);

    let mut seen = BTreeSet::new();
    for (predicate, nodes) in &instance.values {
        if vocab::is_builtin_namespace(predicate) {
            continue;
        }
        let Some(property) = ontology.properties.get(predicate) else {
            issues.push(issue(
                &instance.path,
                predicate,
                "unknown_property",
                Severity::Error,
                format!("unknown property {predicate}"),
            ));
            continue;
        };
        seen.insert(predicate.clone());
        if property.deprecated {
            issues.push(issue(
                &instance.path,
                predicate,
                "deprecated",
                Severity::Warning,
                format!("property {predicate} is deprecated"),
            ));
        }
        check_domain(ontology, instance, property, issues);
        check_range(ontology, instance, property, nodes, issues);
        check_count(ontology, instance, property, nodes.len() as u64, issues);
    }

    for class in &classes {
        for restriction in inherited_restrictions(ontology, class) {
            if seen.contains(&restriction.uri) {
                continue;
            }
            let min = required_min(restriction);
            if min > 0 {
                issues.push(issue(
                    &instance.path,
                    &restriction.uri,
                    "cardinality",
                    Severity::Error,
                    format!("missing required property {} (min {min})", restriction.uri),
                ));
            }
        }
    }
}

fn check_disjoint(ontology: &Ontology, instance: &Instance, issues: &mut Vec<ValidationIssue>) {
    for type_iri in &instance.types {
        let Some(class) = ontology.classes.get(type_iri) else {
            continue;
        };
        for other in &class.disjoint_with {
            if instance.types.iter().any(|item| item == other)
                || ancestors(ontology, &instance.types).contains(other)
            {
                issues.push(issue(
                    &instance.path,
                    type_iri,
                    "disjoint",
                    Severity::Error,
                    format!("{type_iri} is disjoint with {other}"),
                ));
            }
        }
    }
}

fn check_domain(
    ontology: &Ontology,
    instance: &Instance,
    property: &Property,
    issues: &mut Vec<ValidationIssue>,
) {
    let (hard, includes) = split_domain(ontology, property);
    let types = effective_types(ontology, &instance.types);
    if !hard.is_empty()
        && !hard
            .iter()
            .all(|domain| type_matches(ontology, &types, domain))
    {
        issues.push(issue(
            &instance.path,
            &property.uri,
            "domain",
            Severity::Error,
            format!("{} is outside the domain of {}", instance.id, property.uri),
        ));
    } else if hard.is_empty()
        && !includes.is_empty()
        && !includes
            .iter()
            .any(|domain| type_matches(ontology, &types, domain))
        && !types.is_empty()
    {
        issues.push(issue(
            &instance.path,
            &property.uri,
            "domain",
            Severity::Error,
            format!(
                "{} is outside domainIncludes of {}",
                instance.id, property.uri
            ),
        ));
    }
    for expression in &property.domain_expressions {
        if !expression_matches(ontology, &types, expression) {
            issues.push(issue(
                &instance.path,
                &property.uri,
                "domain",
                Severity::Error,
                format!("domain expression of {} is not satisfied", property.uri),
            ));
        }
    }
}

fn split_domain(ontology: &Ontology, property: &Property) -> (Vec<String>, Vec<String>) {
    let mut includes = BTreeSet::new();
    for statement in &ontology.statements {
        if statement.subject != property.uri || statement.kind != crate::model::TermKind::Iri {
            continue;
        }
        if vocab::SCHEMA_DOMAIN.contains(&statement.predicate.as_str()) {
            includes.insert(statement.object.clone());
        }
    }
    let hard = property
        .domain
        .iter()
        .filter(|iri| !includes.contains(*iri))
        .cloned()
        .collect();
    (hard, includes.into_iter().collect())
}

fn check_range(
    ontology: &Ontology,
    instance: &Instance,
    property: &Property,
    nodes: &[NodeValue],
    issues: &mut Vec<ValidationIssue>,
) {
    let ranges = effective_ranges(ontology, property);
    for node in nodes {
        match node {
            NodeValue::Literal { value, datatype } => {
                if property.kind == PropertyType::ObjectProperty
                    && ranges.iter().any(|iri| ontology.classes.contains_key(iri))
                {
                    issues.push(issue(
                        &instance.path,
                        &property.uri,
                        "range",
                        Severity::Error,
                        format!("{} expects a resource, found literal {value}", property.uri),
                    ));
                }
                for range in &ranges {
                    if range.starts_with(vocab::XSD)
                        && !literal_matches(range, value, datatype.as_deref())
                    {
                        issues.push(issue(
                            &instance.path,
                            &property.uri,
                            "range",
                            Severity::Error,
                            format!("{value} is not a valid {}", local_name(range)),
                        ));
                    }
                }
            }
            NodeValue::Iri(iri) => {
                for range in &ranges {
                    if let Some(class) = ontology.classes.get(range) {
                        if !class.one_of.is_empty() && !class.one_of.iter().any(|item| item == iri)
                        {
                            issues.push(issue(
                                &instance.path,
                                &property.uri,
                                "one_of",
                                Severity::Error,
                                format!("{iri} is not in owl:oneOf of {range}"),
                            ));
                        }
                    } else if ontology.schemes.contains_key(range) {
                        let allowed = ontology.concepts.values().any(|concept| {
                            concept.uri == *iri
                                && concept.schemes.iter().any(|scheme| scheme == range)
                        });
                        if !allowed {
                            issues.push(issue(
                                &instance.path,
                                &property.uri,
                                "controlled",
                                Severity::Error,
                                format!("{iri} is not a concept in scheme {range}"),
                            ));
                        }
                    } else if range == vocab::SKOS_CONCEPT && !ontology.concepts.contains_key(iri) {
                        issues.push(issue(
                            &instance.path,
                            &property.uri,
                            "controlled",
                            Severity::Error,
                            format!("{iri} is not a skos:Concept"),
                        ));
                    }
                }
            }
        }
    }
}

fn check_count(
    ontology: &Ontology,
    instance: &Instance,
    property: &Property,
    count: u64,
    issues: &mut Vec<ValidationIssue>,
) {
    let mut min = 0_u64;
    let mut max: Option<u64> = if property.functional { Some(1) } else { None };
    for class_iri in effective_types(ontology, &instance.types) {
        let Some(class) = ontology.classes.get(&class_iri) else {
            continue;
        };
        for restriction in &class.properties {
            if restriction.uri != property.uri {
                continue;
            }
            min = min.max(required_min(restriction));
            if let Some(limit) = restriction.max.or(restriction.exact) {
                max = Some(max.map(|current| current.min(limit)).unwrap_or(limit));
            }
            if restriction.exact.is_some_and(|exact| exact != count) {
                issues.push(issue(
                    &instance.path,
                    &property.uri,
                    "cardinality",
                    Severity::Error,
                    format!(
                        "{} has {count} values, expected {}",
                        property.uri,
                        restriction.exact.unwrap_or(count)
                    ),
                ));
            }
        }
    }
    if count < min {
        issues.push(issue(
            &instance.path,
            &property.uri,
            "cardinality",
            Severity::Error,
            format!("{} has {count} values, min is {min}", property.uri),
        ));
    }
    if let Some(max) = max
        && count > max
    {
        issues.push(issue(
            &instance.path,
            &property.uri,
            "cardinality",
            Severity::Error,
            format!("{} has {count} values, max is {max}", property.uri),
        ));
    }
}

fn required_min(restriction: &PropertyRef) -> u64 {
    if let Some(exact) = restriction.exact {
        return exact;
    }
    if let Some(min) = restriction.min {
        return min;
    }
    if restriction.required
        || restriction.some_values_from.is_some()
        || restriction.has_value.is_some()
    {
        1
    } else {
        0
    }
}

fn inherited_restrictions<'a>(ontology: &'a Ontology, class: &'a Class) -> Vec<&'a PropertyRef> {
    let mut refs = Vec::new();
    let mut pending = vec![class.uri.as_str()];
    let mut seen = BTreeSet::new();
    while let Some(uri) = pending.pop() {
        if !seen.insert(uri.to_string()) {
            continue;
        }
        let Some(current) = ontology.classes.get(uri) else {
            continue;
        };
        refs.extend(current.properties.iter());
        pending.extend(current.parents.iter().map(String::as_str));
    }
    refs
}

fn effective_types(ontology: &Ontology, types: &[String]) -> Vec<String> {
    let mut all = ancestors(ontology, types);
    for iri in types {
        if !all.contains(iri) {
            all.push(iri.clone());
        }
    }
    all.sort();
    all.dedup();
    all
}

fn ancestors(ontology: &Ontology, types: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending: Vec<String> = types.to_vec();
    let mut seen = BTreeSet::new();
    while let Some(uri) = pending.pop() {
        if !seen.insert(uri.clone()) {
            continue;
        }
        found.push(uri.clone());
        if let Some(class) = ontology.classes.get(&uri) {
            pending.extend(class.parents.iter().cloned());
        }
    }
    found
}

fn effective_ranges(ontology: &Ontology, property: &Property) -> Vec<String> {
    if !property.range.is_empty() {
        return property.range.clone();
    }
    let mut pending: Vec<&str> = property
        .super_properties
        .iter()
        .map(String::as_str)
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(uri) = pending.pop() {
        if !seen.insert(uri.to_string()) {
            continue;
        }
        if let Some(parent) = ontology.properties.get(uri) {
            if !parent.range.is_empty() {
                return parent.range.clone();
            }
            pending.extend(parent.super_properties.iter().map(String::as_str));
        }
    }
    Vec::new()
}

fn type_matches(ontology: &Ontology, types: &[String], domain: &str) -> bool {
    if types.iter().any(|iri| iri == domain) {
        return true;
    }
    types.iter().any(|iri| {
        ancestors(ontology, std::slice::from_ref(iri))
            .iter()
            .any(|parent| parent == domain)
    })
}

fn expression_matches(ontology: &Ontology, types: &[String], expression: &ClassExpression) -> bool {
    match expression {
        ClassExpression::Class { iri } => type_matches(ontology, types, iri),
        ClassExpression::Union { operands } => operands
            .iter()
            .any(|operand| expression_matches(ontology, types, operand)),
        ClassExpression::Intersection { operands } => operands
            .iter()
            .all(|operand| expression_matches(ontology, types, operand)),
        ClassExpression::Complement { operand } => !expression_matches(ontology, types, operand),
        ClassExpression::OneOf { individuals: _ } => true,
    }
}

fn literal_matches(datatype: &str, value: &str, actual: Option<&str>) -> bool {
    if let Some(actual) = actual
        && actual != datatype
        && !(datatype.ends_with("#integer") && actual.ends_with("#integer"))
    {
        return false;
    }
    let name = local_name(datatype);
    match name {
        "string" | "normalizedString" => true,
        "integer" | "int" | "long" | "short" | "nonNegativeInteger" | "positiveInteger" => {
            value.parse::<i64>().is_ok()
        }
        "decimal" | "float" | "double" => value.parse::<f64>().is_ok(),
        "boolean" => matches!(value, "true" | "false" | "1" | "0"),
        "date" => value.len() >= 10 && value.as_bytes().get(4) == Some(&b'-'),
        "dateTime" => value.contains('T') || value.contains('t'),
        "anyURI" => value.contains(':') || value.starts_with('/'),
        _ => true,
    }
}

fn resolve_term(ontology: &Ontology, term: &str) -> String {
    if term.starts_with("http://") || term.starts_with("https://") || term.starts_with("_:") {
        return term.to_string();
    }
    let resolved = ontology.resolve_prefix(term);
    if resolved != term {
        return resolved;
    }
    if let Some(class) = ontology.get_class(term) {
        return class.uri.clone();
    }
    if let Some(property) = ontology.get_property(term) {
        return property.uri.clone();
    }
    term.to_string()
}

fn issue(
    path: &str,
    term: &str,
    rule: &str,
    severity: Severity,
    message: String,
) -> ValidationIssue {
    ValidationIssue {
        path: path.to_string(),
        term: term.to_string(),
        rule: rule.to_string(),
        severity,
        message,
    }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn rule_iri(rule: &str) -> String {
    format!("https://ontaria.dev/rules#{rule}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;
    use serde_json::json;

    fn ontology() -> Ontology {
        parse_document(
            br#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
            @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
            ex: a owl:Ontology .
            ex:Agent a owl:Class .
            ex:Person a owl:Class ; rdfs:subClassOf ex:Agent ; owl:disjointWith ex:Org .
            ex:Org a owl:Class .
            ex:News a owl:Class ;
                rdfs:subClassOf [ a owl:Restriction ; owl:onProperty ex:headline ; owl:minCardinality 1 ] .
            ex:headline a owl:DatatypeProperty ; rdfs:domain ex:News ; rdfs:range xsd:string .
            ex:age a owl:DatatypeProperty ; rdfs:domain ex:Person ; rdfs:range xsd:integer .
            ex:status a owl:ObjectProperty ; rdfs:domain ex:News ; rdfs:range ex:Status .
            ex:Status a owl:Class ; owl:oneOf ( ex:live ex:draft ) .
            ex:live a owl:NamedIndividual .
            ex:draft a owl:NamedIndividual .
            ex:old a owl:DatatypeProperty ; owl:deprecated true ; rdfs:domain ex:News ; rdfs:range xsd:string .
            ex:scheme a skos:ConceptScheme .
            ex:sports a skos:Concept ; skos:inScheme ex:scheme .
            ex:topic a owl:ObjectProperty ; rdfs:domain ex:News ; rdfs:range ex:scheme .
            "#,
            "http://example.org/",
        )
        .unwrap()
    }

    #[test]
    fn flags_unknown_terms_cardinality_and_enumerations() {
        let ontology = ontology();
        let report = validate_json(
            &ontology,
            &json!({
                "@type": "ex:News",
                "ex:age": "nope",
                "ex:status": {"@id": "ex:missing"},
                "ex:topic": {"@id": "ex:other"},
                "ex:old": "x",
                "ex:nope": "x"
            }),
        );
        let rules: Vec<_> = report
            .issues
            .iter()
            .map(|issue| issue.rule.as_str())
            .collect();
        assert!(rules.contains(&"cardinality"));
        assert!(rules.contains(&"unknown_property"));
        assert!(rules.contains(&"range"));
        assert!(rules.contains(&"one_of"));
        assert!(rules.contains(&"controlled"));
        assert!(rules.contains(&"deprecated"));
        assert!(report.has_errors());
        let turtle = report.to_turtle();
        assert!(turtle.contains("sh:ValidationReport"));
        assert!(turtle.contains("sh:conforms false"));
    }

    #[test]
    fn accepts_a_valid_news_item_and_rejects_disjoint_types() {
        let ontology = ontology();
        let ok = validate_json(
            &ontology,
            &json!({
                "@id": "ex:item",
                "@type": "ex:News",
                "ex:headline": "Hello",
                "ex:status": {"@id": "ex:live"},
                "ex:topic": {"@id": "ex:sports"}
            }),
        );
        assert!(!ok.has_errors(), "{ok:?}");
        let bad = validate_json(&ontology, &json!({"@type": ["ex:Person", "ex:Org"]}));
        assert!(bad.issues.iter().any(|issue| issue.rule == "disjoint"));
    }
}
