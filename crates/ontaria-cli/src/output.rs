//! JSON envelope shared by every command.
//!
//! Success is `{status, data, warnings}`. Failure is `{status, message}`.
//! Warnings are objects, not bare strings.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use ontaria::{Error, Result, Warning};
use serde::Serialize;
use serde_json::{Value, json};

static LOG_FILE: OnceLock<std::path::PathBuf> = OnceLock::new();

pub fn set_log_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|source| Error::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    LOG_FILE
        .set(path.to_path_buf())
        .map_err(|_| Error::message("log file was already set"))
}

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
    let mut text = serde_json::to_vec(value)
        .map_err(|err| Error::message(format!("failed to write JSON: {err}")))?;
    text.push(b'\n');
    if let Some(path) = LOG_FILE.get() {
        fs::write(path, text).map_err(|source| Error::Write {
            path: path.clone(),
            source,
        })?;
        return Ok(());
    }
    std::io::Write::write_all(&mut std::io::stdout(), &text)
        .map_err(|err| Error::message(format!("failed to write JSON: {err}")))?;
    Ok(())
}
