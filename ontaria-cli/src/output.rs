//! JSON envelope shared by every command.
//!
//! Success is `{status, data, warnings}`. Failure is `{status, message}`.
//! Warnings are objects, not bare strings.

use ontaria::{Error, Result, Warning};
use serde::Serialize;
use serde_json::{Value, json};

pub fn print_success(data: impl Serialize, warnings: &[Warning]) -> Result<()> {
    print_value(&json!({
        "status": "ok",
        "data": data,
        "warnings": warnings,
    }))
}

pub fn print_error(message: impl AsRef<str>) -> Result<()> {
    print_value(&json!({
        "status": "error",
        "message": message.as_ref(),
    }))
}

fn print_value(value: &Value) -> Result<()> {
    serde_json::to_writer(std::io::stdout(), value)
        .map_err(|err| Error::message(format!("failed to write JSON: {err}")))?;
    println!();
    Ok(())
}
