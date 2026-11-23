use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A problem found while parsing or loading that did not stop the load.
///
/// Warnings are data. Callers decide whether to print them, and the CLI can
/// turn them into a failing exit with `lint --deny warnings`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct Warning {
    pub code: WarningCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
}

impl Warning {
    pub fn new(code: WarningCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            term: None,
            file: None,
            line: None,
        }
    }

    pub fn with_term(mut self, term: impl Into<String>) -> Self {
        self.term = Some(term.into());
        self
    }

    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }
}

/// Why a [`Warning`] was recorded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WarningCode {
    CircularImport,
    ImportFailed,
    NamedGraphsMerged,
    RemoteContextSkipped,
    Other,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Failures a caller can report.
///
/// The enum is [`non_exhaustive`] so new context can be added without a major
/// version bump. Match it with a wildcard arm.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to parse {format}{at}: {message}", at = locate(.file, .line))]
    Parse {
        format: String,
        message: String,
        file: Option<String>,
        line: Option<u64>,
    },
    #[error("invalid base IRI '{base}': {reason}")]
    BaseIri { base: String, reason: String },
    #[error("failed to create cache dir {}: {source}", path.display())]
    CacheDir { path: PathBuf, source: io::Error },
    #[error("failed to read {path}: {source}", path = path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {path}: {source}", path = path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("ontology not cached: {iri}")]
    NotCached { iri: String },
    #[error("failed to fetch {url}: {message}")]
    Fetch { url: String, message: String },
    #[error("failed to fetch {url}: HTTP {status}")]
    HttpStatus { url: String, status: u16 },
    #[error("fetch of {url} exceeded the limit: {message}")]
    Limit { url: String, message: String },
    #[error("{kind} not found: {term}")]
    NotFound { kind: String, term: String },
    #[error("lock mismatch for {iri}: expected {expected}, found {actual}")]
    LockMismatch {
        iri: String,
        expected: String,
        actual: String,
    },
    #[error("catalog entry for {iri} does not resolve ({path})", path = path.display())]
    Catalog { iri: String, path: PathBuf },
    #[error("the `{feature}` feature is not enabled: {message}")]
    Feature {
        feature: &'static str,
        message: String,
    },
    #[error("{message}")]
    Message { message: String },
}

fn locate(file: &Option<String>, line: &Option<u64>) -> String {
    match (file, line) {
        (Some(file), Some(line)) => format!(" ({file}:{line})"),
        (Some(file), None) => format!(" ({file})"),
        (None, Some(line)) => format!(" (line {line})"),
        (None, None) => String::new(),
    }
}

impl Error {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message {
            message: message.into(),
        }
    }

    pub fn parse(format: &str, err: impl std::fmt::Display) -> Self {
        Self::Parse {
            format: format.to_string(),
            message: err.to_string(),
            file: None,
            line: None,
        }
    }

    pub fn feature(feature: &'static str, message: impl Into<String>) -> Self {
        Self::Feature {
            feature,
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
