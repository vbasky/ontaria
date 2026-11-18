//! JSON envelope shared by every command.
//!
//! Success is `{status, data}` and includes `warnings` when there are some.
//! Failure is `{status, message}`.

use ontaria::Result;
use serde_json::{Value, json};

pub fn print_success(data: impl serde::Serialize, warnings: &[String]) -> Result<()> {
    let mut value = json!({
        "status": "ok",
        "data": serde_json::to_value(data).map_err(|err| ontaria::Error::Message(err.to_string()))?,
    });
    if !warnings.is_empty() {
        value["warnings"] = serde_json::to_value(warnings)
            .map_err(|err| ontaria::Error::Message(err.to_string()))?;
    }
    print_value(&value)
}

pub fn print_error(message: impl AsRef<str>) -> Result<()> {
    print_value(&json!({
        "status": "error",
        "message": message.as_ref(),
    }))
}

fn print_value(value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| ontaria::Error::Message(err.to_string()))?;
    println!("{text}");
    Ok(())
}
