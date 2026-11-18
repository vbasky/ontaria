use std::io;
use std::path::PathBuf;

/// Failures that a caller can report. Parse and cache errors stay typed so the
/// CLI can turn them into the JSON error envelope.
///
/// The enum is [`non_exhaustive`] so later versions can add context.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("failed to parse turtle: {0}")]
    Parse(String),
    #[error("invalid base IRI '{base}': {reason}")]
    BaseIri { base: String, reason: String },
    #[error("failed to create cache dir {}: {source}", path.display())]
    CacheDir { path: PathBuf, source: io::Error },
    #[error("failed to read {path}: {source}", path = path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {path}: {source}", path = path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("ontology not cached: {0}")]
    NotCached(String),
    #[error("failed to fetch ontology: {0}")]
    Fetch(String),
    #[error("failed to fetch: HTTP {0}")]
    HttpStatus(u16),
    #[error("{0}")]
    Message(String),
    #[error("catalog entry for {iri} does not resolve ({path})", path = path.display())]
    Catalog { iri: String, path: PathBuf },
    #[error("the `{feature}` feature is not enabled: {message}")]
    Feature {
        feature: &'static str,
        message: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
