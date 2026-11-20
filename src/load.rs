//! Load an ontology once, resolving every import before returning.
//!
//! [`Ontology::load`](crate::Ontology::load) is the supported entry point.
//! The value it returns does no further I/O.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cache::{CacheManager, FetchLimits};
use crate::catalog::Catalog;
use crate::error::{Error, Result, Warning, WarningCode};
use crate::lockfile::{LockEntry, LockFile};
use crate::model::{LangPref, Ontology};
use crate::parser::{RdfFormat, detect_format, parse_with_format};

/// Where bytes come from, and which imports to follow.
#[derive(Debug, Clone)]
pub struct LoadOptions {
    pub follow_imports: bool,
    pub cache_dir: Option<PathBuf>,
    pub catalog_path: Option<PathBuf>,
    pub lang: LangPref,
    pub refresh: bool,
    pub locked: bool,
    pub lock_path: Option<PathBuf>,
    /// When false, a missing cache entry is an error instead of a download.
    pub allow_network: bool,
    pub limits: FetchLimits,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            follow_imports: false,
            cache_dir: None,
            catalog_path: None,
            lang: LangPref::default(),
            refresh: false,
            locked: false,
            lock_path: None,
            allow_network: true,
            limits: FetchLimits::default(),
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
    /// `source` is a filesystem path or an `http(s)` URL. Imports are resolved
    /// from the catalog, then a local file, then the cache, then the network
    /// when `allow_network` is set and the `fetch` feature is enabled. The
    /// returned value is immutable: query methods take `&self`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ontaria::{LoadOptions, Ontology};
    /// let ontology = Ontology::load("schema.ttl", &LoadOptions::default())?;
    /// let _shared = std::sync::Arc::new(ontology);
    /// # Ok::<(), ontaria::Error>(())
    /// ```
    pub fn load(source: &str, options: &LoadOptions) -> Result<Self> {
        let cache = match &options.cache_dir {
            Some(dir) => CacheManager::open(dir)?,
            None => CacheManager::shared()?,
        };
        let catalog = match &options.catalog_path {
            Some(path) => Some(Catalog::load(path)?),
            None => None,
        };
        let mut lock = if options.locked || options.lock_path.is_some() {
            LockFile::load(
                options
                    .lock_path
                    .as_deref()
                    .unwrap_or(Path::new("ontaria.lock.json")),
            )?
        } else {
            LockFile::default()
        };
        let mut visited = HashSet::new();
        let mut ontology = load_one(source, options, &cache, catalog.as_ref(), &mut lock, true)?;
        if options.follow_imports {
            let mut pending = ontology.imports.clone();
            let mut index = 0;
            while index < pending.len() {
                let iri = pending[index].clone();
                index += 1;
                if !visited.insert(iri.clone()) {
                    ontology.warnings.push(
                        Warning::new(
                            WarningCode::CircularImport,
                            format!("Circular import detected: {iri}"),
                        )
                        .with_term(iri),
                    );
                    continue;
                }
                match load_one(&iri, options, &cache, catalog.as_ref(), &mut lock, false) {
                    Ok(imported) => {
                        pending.extend(imported.imports.iter().cloned());
                        ontology.absorb(&imported);
                    }
                    Err(err) => {
                        ontology.warnings.push(
                            Warning::new(
                                WarningCode::ImportFailed,
                                format!("Failed to load import {iri}: {err}"),
                            )
                            .with_term(iri),
                        );
                    }
                }
            }
        }
        ontology.apply_language(&options.lang);
        if let Some(path) = &options.lock_path
            && !options.locked
        {
            lock.save(path)?;
        }
        Ok(ontology)
    }
}

fn load_one(
    source: &str,
    options: &LoadOptions,
    cache: &CacheManager,
    catalog: Option<&Catalog>,
    lock: &mut LockFile,
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
    // A remote ontology must not import a file:// URL. A catalog entry,
    // handled above, is the explicit way to point an IRI at a local file.
    if !top_level && source.starts_with("file:") {
        return Err(Error::Fetch {
            url: source.to_string(),
            message: "refusing to read a local file from a remote import".to_string(),
        });
    }
    if is_remote(source) {
        let cached = cache.exists(source);
        if options.refresh || !cached {
            if !options.allow_network {
                return Err(Error::NotCached {
                    iri: source.to_string(),
                });
            }
            cache.fetch_limited(source, &options.limits, options.refresh || cached)?;
        }
        let bytes = cache.load(source)?;
        if options.locked {
            lock.check(source, &bytes)?;
        } else if options.lock_path.is_some() {
            let meta = cache.metadata(source).ok();
            lock.upsert(LockEntry {
                iri: source.to_string(),
                url: source.to_string(),
                sha256: crate::cache::sha256_hex(&bytes),
                fetched_at: meta
                    .as_ref()
                    .map(|item| item.download_time.clone())
                    .unwrap_or_default(),
                etag: meta.and_then(|item| item.etag),
            });
        }
        let format = cache
            .metadata(source)
            .ok()
            .and_then(|meta| meta.format)
            .as_deref()
            .and_then(format_from_name)
            .unwrap_or_else(|| detect_format(&bytes));
        return parse_with_format(&bytes, source, format);
    }
    Err(Error::message(format!(
        "ontology not cached: {source} (run 'fetch' first)"
    )))
}

fn format_from_name(name: &str) -> Option<RdfFormat> {
    match name {
        "turtle" => Some(RdfFormat::Turtle),
        "n-triples" => Some(RdfFormat::NTriples),
        "n-quads" => Some(RdfFormat::NQuads),
        "trig" => Some(RdfFormat::TriG),
        "rdf/xml" => Some(RdfFormat::RdfXml),
        "json-ld" => Some(RdfFormat::JsonLd),
        _ => None,
    }
}

fn read_file(path: &Path, base: &str) -> Result<Ontology> {
    let bytes = fs::read(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let format = path
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(RdfFormat::from_extension)
        .unwrap_or_else(|| detect_format(&bytes));
    parse_with_format(&bytes, base, format)
}

/// Load the ontology selected by `--ontology`.
///
/// An empty spec uses the only cached ontology. A spec that names an existing
/// file is parsed from disk. Anything else is treated as a cache key, which
/// for remote ontologies is the URL passed to `fetch`.
pub fn load_ontology(spec: Option<&str>) -> Result<Loaded> {
    let options = LoadOptions {
        follow_imports: false,
        allow_network: false,
        ..LoadOptions::default()
    };
    let cache = CacheManager::shared()?;
    let mut warnings = Vec::new();
    let spec = match spec.filter(|value| !value.is_empty()) {
        Some(spec) => spec.to_string(),
        None => match cache.list()?.as_slice() {
            [] => {
                return Err(Error::message(
                    "no cached ontologies found, use --ontology or fetch one first",
                ));
            }
            [only] => {
                warnings.push(format!("Using cached ontology: {}", only.url));
                only.url.clone()
            }
            _ => {
                return Err(Error::message(
                    "multiple ontologies cached, specify --ontology",
                ));
            }
        },
    };
    let ontology = if existing_file(&spec).is_some() || is_remote(&spec) {
        Ontology::load(&spec, &options)?
    } else {
        return Err(Error::message(format!(
            "ontology not cached: {spec} (run 'fetch' first)"
        )));
    };
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

/// Parse a duration such as `30d`, `12h`, `15m`, or `45s`.
pub fn parse_age(text: &str) -> Result<Duration> {
    let text = text.trim();
    let (number, unit) = text.split_at(
        text.find(|ch: char| ch.is_ascii_alphabetic())
            .unwrap_or(text.len()),
    );
    let count: u64 = number
        .trim()
        .parse()
        .map_err(|_| Error::message(format!("invalid duration: {text}")))?;
    let seconds = match unit {
        "" | "s" => count,
        "m" => count.saturating_mul(60),
        "h" => count.saturating_mul(60 * 60),
        "d" => count.saturating_mul(60 * 60 * 24),
        _ => {
            return Err(Error::message(format!(
                "invalid duration unit in {text}; use s, m, h, or d"
            )));
        }
    };
    Ok(Duration::from_secs(seconds))
}

#[cfg(feature = "async-fetch")]
pub async fn fetch_bytes(url: &str, limits: &FetchLimits) -> Result<Vec<u8>> {
    if !is_remote(url) {
        return Err(Error::Fetch {
            url: url.to_string(),
            message: "only http and https URLs can be fetched".to_string(),
        });
    }
    let client = reqwest::Client::builder()
        .timeout(limits.timeout)
        .redirect(reqwest::redirect::Policy::limited(
            limits.max_redirects as usize,
        ))
        .build()
        .map_err(|err| Error::Fetch {
            url: url.to_string(),
            message: err.to_string(),
        })?;
    let response = client
        .get(url)
        .header("User-Agent", "ontaria/0.9")
        .send()
        .await
        .map_err(|err| Error::Fetch {
            url: url.to_string(),
            message: err.to_string(),
        })?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(Error::HttpStatus {
            url: url.to_string(),
            status,
        });
    }
    let bytes = response.bytes().await.map_err(|err| Error::Fetch {
        url: url.to_string(),
        message: err.to_string(),
    })?;
    if bytes.len() > limits.max_bytes {
        return Err(Error::Limit {
            url: url.to_string(),
            message: format!("response is larger than {} bytes", limits.max_bytes),
        });
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheManager;
    use crate::lockfile::{LockEntry, LockFile};

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
                .any(|warning| { warning.message.contains("refusing to read a local file") })
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn locked_load_fails_when_the_digest_does_not_match() {
        let dir = std::env::temp_dir().join(format!("ontaria-lock-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = CacheManager::open(dir.join("cache")).unwrap();
        let url = "http://example.org/locked";
        cache
            .save(
                url,
                br#"
                @prefix owl: <http://www.w3.org/2002/07/owl#> .
                <http://example.org/locked> a owl:Ontology .
                "#,
            )
            .unwrap();
        let lock_path = dir.join("ontaria.lock.json");
        let mut lock = LockFile::default();
        lock.upsert(LockEntry {
            iri: url.to_string(),
            url: url.to_string(),
            sha256: "0".repeat(64),
            fetched_at: "2026-01-01T00:00:00Z".to_string(),
            etag: None,
        });
        lock.save(&lock_path).unwrap();
        let options = LoadOptions {
            cache_dir: Some(dir.join("cache")),
            locked: true,
            lock_path: Some(lock_path),
            allow_network: false,
            ..LoadOptions::default()
        };
        let err = Ontology::load(url, &options).unwrap_err();
        assert!(matches!(err, Error::LockMismatch { .. }));
        let _ = fs::remove_dir_all(&dir);
    }
}
