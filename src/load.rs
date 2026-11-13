//! Decide where an ontology's bytes come from.
//!
//! A command may name an HTTP(S) URL, which must already be in the cache
//! unless an import fetch is allowed later, or a filesystem path, which is
//! parsed directly.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::CacheManager;
use crate::error::{Error, Result};
use crate::model::Ontology;
use crate::parser::parse_document;

pub struct Loaded {
    pub ontology: Ontology,
    pub warnings: Vec<String>,
    pub cache: CacheManager,
}

/// Load the ontology selected by `--ontology`.
///
/// An empty spec uses the only cached ontology. A spec that names an existing
/// file is parsed from disk. Anything else is treated as a cache key, which
/// for remote ontologies is the URL passed to `fetch`.
pub fn load_ontology(spec: Option<&str>) -> Result<Loaded> {
    let cache = CacheManager::shared()?;
    let mut warnings = Vec::new();
    let spec = match spec.filter(|value| !value.is_empty()) {
        Some(spec) => spec.to_string(),
        None => match cache.list()?.as_slice() {
            [] => {
                return Err(Error::Message(
                    "no cached ontologies found, use --ontology or fetch one first".to_string(),
                ));
            }
            [only] => {
                warnings.push(format!("Using cached ontology: {}", only.url));
                only.url.clone()
            }
            _ => {
                return Err(Error::Message(
                    "multiple ontologies cached, specify --ontology".to_string(),
                ));
            }
        },
    };

    if let Some(path) = existing_file(&spec) {
        let bytes = fs::read(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        let base = file_url(&path);
        let ontology = parse_document(bytes.as_slice(), &base)?;
        return Ok(Loaded {
            ontology,
            warnings,
            cache,
        });
    }

    if !cache.exists(&spec) {
        return Err(Error::Message(format!(
            "ontology not cached: {spec} (run 'fetch' first)"
        )));
    }
    let bytes = cache.load(&spec)?;
    let ontology = parse_document(bytes.as_slice(), &spec)?;
    Ok(Loaded {
        ontology,
        warnings,
        cache,
    })
}

pub fn existing_file(spec: &str) -> Option<PathBuf> {
    if spec.starts_with("http://") || spec.starts_with("https://") {
        return None;
    }
    let path = Path::new(spec);
    if path.is_file() {
        Some(path.to_path_buf())
    } else {
        None
    }
}

pub fn file_url(path: &Path) -> String {
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", absolute.display())
}
