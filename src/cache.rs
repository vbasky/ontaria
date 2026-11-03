//! On-disk cache of ontology bytes.
//!
//! Files are `{sha256(url)}.ttl` plus `{sha256(url)}.meta.json` under
//! `~/.vocable/cache`.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheMetadata {
    pub url: String,
    pub download_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CacheManager {
    dir: PathBuf,
}

impl CacheManager {
    /// Open the shared user cache, creating it when it is missing.
    pub fn shared() -> Result<Self> {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            Error::Message(
                "HOME is not set, so the ontology cache path cannot be built".to_string(),
            )
        })?;
        Self::open(Path::new(&home).join(".vocable").join("cache"))
    }

    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir).map_err(|source| Error::CacheDir {
            path: dir.clone(),
            source,
        })?;
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn exists(&self, url: &str) -> bool {
        self.content_path(url).is_file()
    }

    pub fn save(&self, url: &str, content: &[u8]) -> Result<CacheMetadata> {
        let content_path = self.content_path(url);
        fs::write(&content_path, content).map_err(|source| Error::Write {
            path: content_path,
            source,
        })?;
        let meta = CacheMetadata {
            url: url.to_string(),
            download_time: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            etag: None,
        };
        let meta_path = self.meta_path(url);
        let bytes = serde_json::to_vec_pretty(&meta)
            .map_err(|err| Error::Message(format!("failed to encode cache metadata: {err}")))?;
        fs::write(&meta_path, bytes).map_err(|source| Error::Write {
            path: meta_path,
            source,
        })?;
        Ok(meta)
    }

    pub fn load(&self, url: &str) -> Result<Vec<u8>> {
        let path = self.content_path(url);
        fs::read(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::NotCached(url.to_string())
            } else {
                Error::Read { path, source }
            }
        })
    }

    pub fn metadata(&self, url: &str) -> Result<CacheMetadata> {
        let path = self.meta_path(url);
        let bytes = fs::read(&path).map_err(|source| Error::Read { path, source })?;
        serde_json::from_slice(&bytes)
            .map_err(|err| Error::Message(format!("failed to parse metadata for {url}: {err}")))
    }

    pub fn fetch(&self, url: &str) -> Result<CacheMetadata> {
        let mut response = ureq::get(url)
            .header("User-Agent", "vocable/0.1")
            .header("Accept", "text/turtle, application/rdf+xml, */*")
            .call()
            .map_err(|err| Error::Fetch(err.to_string()))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(Error::HttpStatus(status));
        }
        let bytes = response
            .body_mut()
            .read_to_vec()
            .map_err(|err| Error::Fetch(format!("failed to read response: {err}")))?;
        self.save(url, &bytes)
    }

    pub fn list(&self) -> Result<Vec<CacheMetadata>> {
        let entries = fs::read_dir(&self.dir).map_err(|source| Error::Read {
            path: self.dir.clone(),
            source,
        })?;
        let mut found = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| Error::Read {
                path: self.dir.clone(),
                source,
            })?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".meta.json") {
                continue;
            }
            let bytes = fs::read(entry.path()).map_err(|source| Error::Read {
                path: entry.path(),
                source,
            })?;
            if let Ok(meta) = serde_json::from_slice::<CacheMetadata>(&bytes) {
                found.push(meta);
            }
        }
        found.sort_by(|left, right| left.url.cmp(&right.url));
        Ok(found)
    }

    fn content_path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.ttl", hash_url(url)))
    }

    fn meta_path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.meta.json", hash_url(url)))
    }
}

fn hash_url(url: &str) -> String {
    let digest = Sha256::digest(url.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_uses_the_url_hash_as_the_file_name() {
        let dir = std::env::temp_dir().join(format!("vocable-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = CacheManager::open(&dir).unwrap();
        let url = "https://example.org/demo.ttl";
        cache
            .save(url, b"@prefix : <http://example.org/> .")
            .unwrap();
        assert!(cache.exists(url));
        assert_eq!(
            cache.load(url).unwrap(),
            b"@prefix : <http://example.org/> ."
        );
        assert_eq!(cache.metadata(url).unwrap().url, url);
        assert_eq!(cache.list().unwrap().len(), 1);
        assert!(
            cache
                .content_path(url)
                .ends_with(format!("{}.ttl", hash_url(url)))
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
