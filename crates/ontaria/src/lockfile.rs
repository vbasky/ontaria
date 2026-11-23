//! A lock file records the bytes an import resolved to.
//!
//! `--locked` fails when the fetched SHA-256 does not match the entry.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cache::sha256_hex;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockFile {
    pub version: u32,
    pub entries: Vec<LockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockEntry {
    pub iri: String,
    pub url: String,
    pub sha256: String,
    pub fetched_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            version: 1,
            entries: Vec::new(),
        }
    }
}

impl LockFile {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.is_file() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|err| Error::message(format!("failed to parse lock file: {err}")))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut copy = self.clone();
        copy.entries.sort_by(|left, right| left.iri.cmp(&right.iri));
        let path = path.as_ref();
        let bytes = serde_json::to_vec_pretty(&copy)
            .map_err(|err| Error::message(format!("failed to encode lock file: {err}")))?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|source| Error::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(path, bytes).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn get(&self, iri: &str) -> Option<&LockEntry> {
        self.entries.iter().find(|entry| entry.iri == iri)
    }

    pub fn upsert(&mut self, entry: LockEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|item| item.iri == entry.iri) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn check(&self, iri: &str, bytes: &[u8]) -> Result<()> {
        let Some(entry) = self.get(iri) else {
            return Err(Error::LockMismatch {
                iri: iri.to_string(),
                expected: "an entry in the lock file".to_string(),
                actual: "no entry".to_string(),
            });
        };
        let actual = sha256_hex(bytes);
        if actual != entry.sha256 {
            return Err(Error::LockMismatch {
                iri: iri.to_string(),
                expected: entry.sha256.clone(),
                actual,
            });
        }
        Ok(())
    }
}

#[allow(dead_code)]
pub fn read_optional(path: &Option<PathBuf>) -> Result<LockFile> {
    match path {
        Some(path) => LockFile::load(path),
        None => Ok(LockFile::default()),
    }
}
