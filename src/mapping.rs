//! A small YAML mapping from JSON objects to JSON-LD.
//!
//! The profile is intentionally smaller than RML. A document names the
//! subject field, the RDF type, and a list of source paths. Nested
//! `mappings` build blank nodes. `map validate` checks each target term
//! against a loaded ontology. `map apply` writes JSON-LD, optionally with a
//! PROV-O derivation statement. `reverse` reads that JSON-LD back into the
//! source shape.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::model::{Ontology, PropertyType};
use crate::validate::{Severity, ValidationIssue, ValidationReport};
use crate::vocab::{self, local_name};

/// One YAML mapping document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mapping {
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub prefixes: BTreeMap<String, String>,
    /// JSON field, or a `$.` path, whose value is the subject IRI.
    pub subject: String,
    /// RDF type of each source object. An IRI or `prefix:Name`.
    #[serde(rename = "type")]
    pub type_iri: String,
    #[serde(default)]
    pub mappings: Vec<FieldMapping>,
    /// When set, each output node records `prov:wasDerivedFrom`.
    #[serde(default)]
    pub provenance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldMapping {
    pub source: String,
    pub property: String,
    /// `literal` (default) or `iri`.
    #[serde(default = "literal_type")]
    #[serde(rename = "type")]
    pub value_type: String,
    #[serde(default)]
    pub datatype: Option<String>,
    #[serde(default)]
    pub mappings: Vec<FieldMapping>,
}

fn literal_type() -> String {
    "literal".to_string()
}

use std::collections::BTreeMap;

/// Parse a YAML mapping document.
pub fn load_mapping(text: &str) -> Result<Mapping> {
    serde_yaml::from_str(text).map_err(|err| Error::message(format!("invalid mapping: {err}")))
}

/// Check that every target term exists and that domain and range fit `type`.
pub fn validate_mapping(ontology: &Ontology, mapping: &Mapping) -> ValidationReport {
    let mut issues = Vec::new();
    let type_iri = expand(mapping, &mapping.type_iri);
    if ontology.get_class(&type_iri).is_none() && !vocab::is_builtin_namespace(&type_iri) {
        issues.push(problem(
            &mapping.type_iri,
            "unknown_type",
            &format!("mapping type {type_iri} is not a class"),
        ));
    }
    check_fields(ontology, mapping, &type_iri, &mapping.mappings, &mut issues);
    ValidationReport { issues }
}

fn check_fields(
    ontology: &Ontology,
    mapping: &Mapping,
    type_iri: &str,
    fields: &[FieldMapping],
    issues: &mut Vec<ValidationIssue>,
) {
    for field in fields {
        let property_iri = expand(mapping, &field.property);
        let Some(property) = ontology.get_property(&property_iri) else {
            issues.push(problem(
                &field.property,
                "unknown_property",
                &format!("mapping property {property_iri} is not defined"),
            ));
            continue;
        };
        if !property.domain.is_empty()
            && !property.domain.iter().any(|domain| {
                domain == type_iri
                    || ontology
                        .classes
                        .get(type_iri)
                        .is_some_and(|class| class.parents.iter().any(|parent| parent == domain))
            })
        {
            issues.push(problem(
                &field.property,
                "domain",
                &format!("{property_iri} is not in the domain of {type_iri}"),
            ));
        }
        if field.value_type == "iri" && property.kind == PropertyType::DatatypeProperty {
            issues.push(problem(
                &field.property,
                "range",
                &format!("{property_iri} is a datatype property and cannot take an IRI"),
            ));
        }
        if field.value_type != "iri"
            && property.kind == PropertyType::ObjectProperty
            && field.mappings.is_empty()
        {
            issues.push(problem(
                &field.property,
                "range",
                &format!("{property_iri} expects a resource"),
            ));
        }
        if let Some(datatype) = &field.datatype {
            let datatype = expand(mapping, datatype);
            if property
                .range
                .iter()
                .any(|range| range.starts_with(vocab::XSD))
                && !property.range.iter().any(|range| range == &datatype)
            {
                issues.push(problem(
                    &field.property,
                    "range",
                    &format!("{property_iri} range does not include {datatype}"),
                ));
            }
        }
        if !field.mappings.is_empty() {
            let nested = property
                .range
                .first()
                .cloned()
                .unwrap_or_else(|| type_iri.to_string());
            check_fields(ontology, mapping, &nested, &field.mappings, issues);
        }
    }
}

/// Apply `mapping` to a JSON object or array of objects.
pub fn apply(mapping: &Mapping, value: &Value) -> Result<Value> {
    match value {
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                out.push(apply_one(mapping, item)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(_) => apply_one(mapping, value),
        _ => Err(Error::message(
            "mapping input must be a JSON object or array",
        )),
    }
}

fn apply_one(mapping: &Mapping, value: &Value) -> Result<Value> {
    let Some(object) = value.as_object() else {
        return Err(Error::message("mapping input must be a JSON object"));
    };
    let subject = lookup(object, &mapping.subject)
        .and_then(|item| item.as_str())
        .ok_or_else(|| Error::message(format!("missing subject field {}", mapping.subject)))?;
    let mut node = Map::new();
    node.insert(
        "@id".to_string(),
        Value::String(expand_subject(mapping, subject)),
    );
    node.insert(
        "@type".to_string(),
        Value::String(expand(mapping, &mapping.type_iri)),
    );
    write_fields(mapping, object, &mapping.mappings, &mut node)?;
    if mapping.provenance {
        node.insert(
            "prov:wasDerivedFrom".to_string(),
            json!({ "@id": expand_subject(mapping, subject) }),
        );
    }
    let mut context = Map::new();
    for (prefix, iri) in &mapping.prefixes {
        context.insert(prefix.clone(), Value::String(iri.clone()));
    }
    if mapping.provenance {
        context.insert("prov".to_string(), Value::String(vocab::PROV.to_string()));
    }
    node.insert("@context".to_string(), Value::Object(context));
    Ok(Value::Object(node))
}

fn write_fields(
    mapping: &Mapping,
    object: &Map<String, Value>,
    fields: &[FieldMapping],
    into: &mut Map<String, Value>,
) -> Result<()> {
    for field in fields {
        let Some(source) = lookup(object, &field.source) else {
            continue;
        };
        let key = predicate_key(mapping, &field.property);
        let value = if !field.mappings.is_empty() {
            let nested_object = source
                .as_object()
                .ok_or_else(|| Error::message(format!("{} is not an object", field.source)))?;
            let mut nested = Map::new();
            write_fields(mapping, nested_object, &field.mappings, &mut nested)?;
            Value::Object(nested)
        } else if field.value_type == "iri" {
            let text = source.as_str().unwrap_or(&source.to_string()).to_string();
            json!({ "@id": expand_subject(mapping, &text) })
        } else if let Some(datatype) = &field.datatype {
            json!({
                "@value": json_literal(source),
                "@type": expand(mapping, datatype)
            })
        } else {
            json_literal(source)
        };
        into.insert(key, value);
    }
    Ok(())
}

/// Rebuild the source JSON from JSON-LD produced by [`apply`].
pub fn reverse(mapping: &Mapping, value: &Value) -> Result<Value> {
    match value {
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                out.push(reverse_one(mapping, item)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(_) => reverse_one(mapping, value),
        _ => Err(Error::message(
            "reverse input must be a JSON object or array",
        )),
    }
}

fn reverse_one(mapping: &Mapping, value: &Value) -> Result<Value> {
    let Some(object) = value.as_object() else {
        return Err(Error::message("reverse input must be a JSON object"));
    };
    let mut out = Map::new();
    if let Some(id) = object.get("@id").and_then(|item| item.as_str()) {
        out.insert(
            field_name(&mapping.subject),
            Value::String(local_of(mapping, id)),
        );
    }
    read_fields(mapping, object, &mapping.mappings, &mut out);
    Ok(Value::Object(out))
}

fn read_fields(
    mapping: &Mapping,
    object: &Map<String, Value>,
    fields: &[FieldMapping],
    into: &mut Map<String, Value>,
) {
    for field in fields {
        let key = predicate_key(mapping, &field.property);
        let Some(value) = object
            .get(&key)
            .or_else(|| object.get(&expand(mapping, &field.property)))
        else {
            continue;
        };
        let stored = if !field.mappings.is_empty() {
            let Some(nested) = value.as_object() else {
                continue;
            };
            let mut child = Map::new();
            read_fields(mapping, nested, &field.mappings, &mut child);
            Value::Object(child)
        } else if let Some(id) = value.get("@id").and_then(|item| item.as_str()) {
            Value::String(local_of(mapping, id))
        } else if let Some(text) = value.get("@value") {
            text.clone()
        } else {
            value.clone()
        };
        into.insert(field_name(&field.source), stored);
    }
}

fn problem(term: &str, rule: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        path: term.to_string(),
        term: term.to_string(),
        rule: rule.to_string(),
        severity: Severity::Error,
        message: message.to_string(),
    }
}

fn expand(mapping: &Mapping, term: &str) -> String {
    if term.starts_with("http://") || term.starts_with("https://") {
        return term.to_string();
    }
    if let Some((prefix, local)) = term.split_once(':')
        && let Some(namespace) = mapping.prefixes.get(prefix)
    {
        return format!("{namespace}{local}");
    }
    if mapping.base.is_empty() {
        term.to_string()
    } else {
        format!("{}{}", mapping.base, term.trim_start_matches(':'))
    }
}

fn expand_subject(mapping: &Mapping, value: &str) -> String {
    if value.starts_with("http://") || value.starts_with("https://") || mapping.base.is_empty() {
        value.to_string()
    } else {
        format!("{}{}", mapping.base, value.trim_start_matches('/'))
    }
}

fn predicate_key(mapping: &Mapping, property: &str) -> String {
    if let Some((prefix, local)) = property.split_once(':')
        && mapping.prefixes.contains_key(prefix)
    {
        return format!("{prefix}:{local}");
    }
    local_name(&expand(mapping, property)).to_string()
}

fn field_name(source: &str) -> String {
    source.trim_start_matches("$.").to_string()
}

fn lookup<'a>(object: &'a Map<String, Value>, source: &str) -> Option<&'a Value> {
    let mut current = object;
    let path = source.trim_start_matches("$.");
    let mut parts = path.split('.');
    let first = parts.next()?;
    let mut value = current.get(first)?;
    for part in parts {
        current = value.as_object()?;
        value = current.get(part)?;
    }
    Some(value)
}

fn json_literal(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(text.clone()),
        other => other.clone(),
    }
}

fn local_of(mapping: &Mapping, iri: &str) -> String {
    if !mapping.base.is_empty()
        && let Some(rest) = iri.strip_prefix(&mapping.base)
    {
        return rest.to_string();
    }
    local_name(iri).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;
    use serde_json::json;

    const YAML: &str = r#"
base: http://example.org/
prefixes:
  ex: http://example.org/
subject: id
type: ex:Person
provenance: true
mappings:
  - source: name
    property: ex:name
    datatype: http://www.w3.org/2001/XMLSchema#string
  - source: city
    property: ex:city
"#;

    fn ontology() -> Ontology {
        parse_document(
            br#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
            ex:Person a owl:Class .
            ex:name a owl:DatatypeProperty ; rdfs:domain ex:Person ; rdfs:range xsd:string .
            ex:city a owl:DatatypeProperty ; rdfs:domain ex:Person ; rdfs:range xsd:string .
            "#,
            "http://example.org/",
        )
        .unwrap()
    }

    #[test]
    fn validates_applies_and_reverses() {
        let mapping = load_mapping(YAML).unwrap();
        let report = validate_mapping(&ontology(), &mapping);
        assert!(!report.has_errors(), "{report:?}");
        let output = apply(
            &mapping,
            &json!({"id": "ada", "name": "Ada", "city": "London"}),
        )
        .unwrap();
        assert_eq!(output["@id"], "http://example.org/ada");
        assert_eq!(output["@type"], "http://example.org/Person");
        assert_eq!(output["ex:name"]["@value"], "Ada");
        assert!(output.get("prov:wasDerivedFrom").is_some());
        let back = reverse(&mapping, &output).unwrap();
        assert_eq!(back["id"], "ada");
        assert_eq!(back["name"], "Ada");
        assert_eq!(back["city"], "London");
    }
}
