//! Browser and Node bindings. The crate does not fetch the network.
//!
//! Build for the web with `wasm-pack build --target web crates/ontaria-wasm`.
//! The host `cdylib` compiles without a wasm32 target so CI can type-check it.

use wasm_bindgen::prelude::*;

/// Parse an ontology and return a JSON list of class IRIs.
#[wasm_bindgen]
pub fn parse(text: &str, base: &str) -> Result<String, JsValue> {
    let ontology = ontaria::parse_document(text.as_bytes(), base).map_err(js_error)?;
    let classes: Vec<&str> = ontology.classes.keys().map(String::as_str).collect();
    serde_json::to_string(&classes).map_err(js_error)
}

/// Label of one class, or an empty string when the term is missing.
#[wasm_bindgen]
pub fn class_label(text: &str, base: &str, term: &str) -> Result<String, JsValue> {
    let ontology = ontaria::parse_document(text.as_bytes(), base).map_err(js_error)?;
    Ok(ontology
        .get_class(term)
        .map(|class| class.label.clone())
        .unwrap_or_default())
}

fn js_error(err: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&err.to_string())
}
