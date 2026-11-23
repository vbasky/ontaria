//! Python module `ontaria_python`.
//!
//! The binding does not download ontologies. Pass Turtle, RDF/XML, or JSON-LD
//! text to [`parse`] or [`class_label`].

use pyo3::prelude::*;

/// Parse an ontology and return JSON with class and property IRIs.
#[pyfunction]
fn parse(text: &str, base: &str) -> PyResult<String> {
    let ontology = ontaria::parse_document(text.as_bytes(), base).map_err(value_error)?;
    let value = serde_json::json!({
        "uri": ontology.uri,
        "classes": ontology.classes.keys().cloned().collect::<Vec<_>>(),
        "properties": ontology.properties.keys().cloned().collect::<Vec<_>>(),
    });
    Ok(value.to_string())
}

/// Return the preferred label of a class.
#[pyfunction]
fn class_label(text: &str, base: &str, term: &str) -> PyResult<Option<String>> {
    let ontology = ontaria::parse_document(text.as_bytes(), base).map_err(value_error)?;
    Ok(ontology.get_class(term).map(|class| class.label.clone()))
}

fn value_error(err: impl std::fmt::Display) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(err.to_string())
}

#[pymodule]
fn ontaria_python(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(parse, module)?)?;
    module.add_function(wrap_pyfunction!(class_label, module)?)?;
    Ok(())
}
