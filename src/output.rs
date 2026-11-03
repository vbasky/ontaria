use serde::Serialize;
use serde_json::{Value, json};

use crate::error::Result;

/// Print `{ "status": "ok", "data": ... }`. `warnings` is included only when
/// there are some.
pub fn print_success(data: impl Serialize, warnings: &[String]) -> Result<()> {
    let mut value = json!({
        "status": "ok",
        "data": serde_json::to_value(data).map_err(|err| crate::Error::Message(err.to_string()))?,
    });
    if !warnings.is_empty() {
        value["warnings"] =
            serde_json::to_value(warnings).map_err(|err| crate::Error::Message(err.to_string()))?;
    }
    print_value(&value)
}

/// Print `{"status":"error","message":...}` and nothing else.
pub fn print_error(message: impl AsRef<str>) -> Result<()> {
    print_value(&json!({
        "status": "error",
        "message": message.as_ref(),
    }))
}

fn print_value(value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| crate::Error::Message(err.to_string()))?;
    println!("{text}");
    Ok(())
}
