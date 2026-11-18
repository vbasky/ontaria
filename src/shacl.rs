//! SHACL validation behind the `shacl` feature.
//!
//! The engine is `shacl-rust` (MIT). Ontaria keeps its own `oxrdf` types, so
//! graphs are handed across as N-Triples. Without the feature,
//! [`validate_shacl`] returns [`Error::Feature`](crate::Error::Feature).

use crate::error::{Error, Result};
use crate::model::Ontology;
use crate::validate::ValidationReport;
#[cfg(feature = "shacl")]
use crate::validate::{Severity, ValidationIssue};

/// Validate `data` against `shapes`.
///
/// Both documents are ontologies ontaria has already parsed. The shapes
/// document is the one that contains `sh:NodeShape` triples.
pub fn validate_shacl(shapes: &Ontology, data: &Ontology) -> Result<ValidationReport> {
    #[cfg(feature = "shacl")]
    {
        validate_with_engine(shapes, data)
    }
    #[cfg(not(feature = "shacl"))]
    {
        let _ = (shapes, data);
        Err(Error::feature(
            "shacl",
            "rebuild ontaria with `--features shacl` to validate SHACL shapes",
        ))
    }
}

#[cfg(feature = "shacl")]
fn validate_with_engine(shapes: &Ontology, data: &Ontology) -> Result<ValidationReport> {
    use shacl_rust::{parse_shapes, validate, validation::dataset::ValidationDataset};

    let shapes_graph = graph_from(shapes)?;
    let data_graph = graph_from(data)?;
    let dataset = ValidationDataset::from_graphs(data_graph, shapes_graph)
        .map_err(|err| Error::message(format!("failed to build a SHACL dataset: {err}")))?;
    let parsed = parse_shapes(dataset.shapes_graph())
        .map_err(|err| Error::message(format!("failed to parse SHACL shapes: {err}")))?;
    let report = validate(&dataset, &parsed);
    let mut issues = Vec::new();
    for result in report.get_results() {
        let message = result
            .messages()
            .first()
            .cloned()
            .unwrap_or_else(|| "SHACL violation".to_string());
        issues.push(ValidationIssue {
            path: result.focus_node().to_string(),
            term: result.source_shape().to_string(),
            rule: "shacl".to_string(),
            severity: Severity::Error,
            message,
        });
    }
    Ok(ValidationReport { issues })
}

#[cfg(feature = "shacl")]
fn graph_from(ontology: &Ontology) -> Result<oxigraph::model::Graph> {
    use oxigraph::io::{RdfFormat, RdfParser};
    use oxigraph::model::Graph;

    let text = to_ntriples(ontology);
    let mut graph = Graph::new();
    let parser = RdfParser::from_format(RdfFormat::NTriples).for_slice(text.as_bytes());
    for quad in parser {
        let quad = quad.map_err(|err| Error::parse("n-triples", err))?;
        let triple = oxigraph::model::Triple::new(quad.subject, quad.predicate, quad.object);
        graph.insert(&triple);
    }
    Ok(graph)
}

#[cfg(feature = "shacl")]
fn to_ntriples(ontology: &Ontology) -> String {
    let mut lines = Vec::new();
    for statement in &ontology.statements {
        if statement.inferred {
            continue;
        }
        let Some(subject) = nt_node(&statement.subject) else {
            continue;
        };
        let predicate = format!("<{}>", statement.predicate);
        let object = match statement.kind {
            crate::model::TermKind::Iri => format!("<{}>", statement.object),
            crate::model::TermKind::Blank => {
                let id = statement.object.trim_start_matches("_:");
                format!("_:{id}")
            }
            crate::model::TermKind::Literal => {
                let escaped = statement
                    .object
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n");
                if let Some(language) = &statement.language {
                    format!("\"{escaped}\"@{language}")
                } else if let Some(datatype) = &statement.datatype {
                    format!("\"{escaped}\"^^<{datatype}>")
                } else {
                    format!("\"{escaped}\"")
                }
            }
        };
        lines.push(format!("{subject} {predicate} {object} ."));
    }
    lines.join("\n")
}

#[cfg(feature = "shacl")]
fn nt_node(id: &str) -> Option<String> {
    if let Some(blank) = id.strip_prefix("_:") {
        Some(format!("_:{blank}"))
    } else if id.starts_with("http://") || id.starts_with("https://") {
        Some(format!("<{id}>"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Ontology;

    #[test]
    fn feature_gate_is_explicit() {
        let shapes = Ontology::new("http://example.org/shapes");
        let data = Ontology::new("http://example.org/data");
        let result = validate_shacl(&shapes, &data);
        #[cfg(not(feature = "shacl"))]
        assert!(matches!(
            result,
            Err(Error::Feature {
                feature: "shacl",
                ..
            })
        ));
        #[cfg(feature = "shacl")]
        {
            let _ = result;
        }
    }
}
