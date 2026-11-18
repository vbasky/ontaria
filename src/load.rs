//! Load an ontology once, resolving every import before returning.
//!
//! [`Ontology::load`](crate::Ontology::load) is the supported entry point.
//! The value it returns does no further I/O.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::CacheManager;
use crate::catalog::Catalog;
use crate::error::{Error, Result};
use crate::model::Ontology;
use crate::parser::parse_document;

/// Where bytes come from, and which imports to follow.
#[derive(Debug, Clone)]
pub struct LoadOptions {
    pub follow_imports: bool,
    pub cache_dir: Option<PathBuf>,
    pub catalog_path: Option<PathBuf>,
    /// When false, a missing cache entry is an error instead of a download.
    pub allow_network: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            follow_imports: false,
            cache_dir: None,
            catalog_path: None,
            allow_network: true,
        }
    }
}

pub struct Loaded {
    pub ontology: Ontology,
    pub warnings: Vec<String>,
    pub cache: CacheManager,
}

impl Ontology {
    /// Load `source` and, when asked, every import it reaches.
    ///
    /// `source` is a filesystem path or an `http(s)` URL. The returned value
    /// is immutable: query methods take `&self`.
    pub fn load(source: &str, options: &LoadOptions) -> Result<Self> {
        let cache = match &options.cache_dir {
            Some(dir) => CacheManager::open(dir)?,
            None => CacheManager::shared()?,
        };
        let catalog = match &options.catalog_path {
            Some(path) => Some(Catalog::load(path)?),
            None => None,
        };
        let mut visited = HashSet::new();
        let mut ontology = load_one(source, options, &cache, catalog.as_ref(), true)?;
        if options.follow_imports {
            let mut pending = ontology.imports.clone();
            let mut index = 0;
            while index < pending.len() {
                let iri = pending[index].clone();
                index += 1;
                if !visited.insert(iri.clone()) {
                    ontology
                        .warnings
                        .push(format!("Circular import detected: {iri}"));
                    continue;
                }
                match load_one(&iri, options, &cache, catalog.as_ref(), false) {
                    Ok(imported) => {
                        pending.extend(imported.imports.iter().cloned());
                        ontology.absorb(&imported);
                    }
                    Err(err) => {
                        ontology
                            .warnings
                            .push(format!("Failed to load import {iri}: {err}"));
                    }
                }
            }
        }
        Ok(ontology)
    }
}

fn load_one(
    source: &str,
    options: &LoadOptions,
    cache: &CacheManager,
    catalog: Option<&Catalog>,
    top_level: bool,
) -> Result<Ontology> {
    if let Some(catalog) = catalog
        && let Some(path) = catalog.resolve(source)
    {
        if !path.is_file() {
            return Err(Error::Catalog {
                iri: source.to_string(),
                path: path.to_path_buf(),
            });
        }
        return read_file(path, source);
    }
    if let Some(path) = existing_file(source) {
        return read_file(&path, &file_url(&path));
    }
    if !top_level && source.starts_with("file:") {
        return Err(Error::Fetch(format!(
            "{source}: refusing to read a local file from a remote import"
        )));
    }
    if is_remote(source) {
        if !cache.exists(source) {
            if !options.allow_network {
                return Err(Error::NotCached(source.to_string()));
            }
            cache.fetch(source)?;
        }
        let bytes = cache.load(source)?;
        return parse_document(bytes.as_slice(), source);
    }
    Err(Error::Message(format!(
        "ontology not cached: {source} (run 'fetch' first)"
    )))
}

fn read_file(path: &Path, base: &str) -> Result<Ontology> {
    let bytes = fs::read(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse_document(bytes.as_slice(), base)
}

/// Load the ontology selected by `--ontology`. Query commands pass
/// `allow_network: false` so a missing cache entry is not downloaded.
pub fn load_ontology(spec: Option<&str>, options: &LoadOptions) -> Result<Loaded> {
    let cache = match &options.cache_dir {
        Some(dir) => CacheManager::open(dir)?,
        None => CacheManager::shared()?,
    };
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
    let mut ontology = if existing_file(&spec).is_some() || is_remote(&spec) {
        Ontology::load(&spec, options)?
    } else {
        return Err(Error::Message(format!(
            "ontology not cached: {spec} (run 'fetch' first)"
        )));
    };
    warnings.append(&mut ontology.warnings);
    ontology.warnings.clone_from(&warnings);
    Ok(Loaded {
        ontology,
        warnings,
        cache,
    })
}

pub fn existing_file(spec: &str) -> Option<PathBuf> {
    if is_remote(spec) || spec.starts_with("file:") {
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

pub fn is_remote(spec: &str) -> bool {
    spec.starts_with("http://") || spec.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_import_refuses_a_file_url() {
        let dir = std::env::temp_dir().join(format!("ontaria-file-import-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.ttl");
        fs::write(
            &path,
            br#"
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            <http://example.org/main> a owl:Ontology ;
                owl:imports <file:///tmp/ontaria-secret.ttl> .
            "#,
        )
        .unwrap();
        let options = LoadOptions {
            follow_imports: true,
            cache_dir: Some(dir.join("cache")),
            allow_network: false,
            ..LoadOptions::default()
        };
        let ontology = Ontology::load(path.to_str().unwrap(), &options).unwrap();
        assert!(
            ontology
                .warnings
                .iter()
                .any(|warning| { warning.contains("refusing to read a local file") })
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ontology_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Ontology>();
    }
}
