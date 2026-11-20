//! On-disk cache of ontology bytes.
//!
//! Files are `{sha256(url)}.{ext}` plus `{sha256(url)}.meta.json`. The
//! directory comes from `--cache-dir`, then `ONTARIA_CACHE_DIR`, then the
//! platform cache (`$XDG_CACHE_HOME/ontaria` or `~/.cache/ontaria`). An
//! existing `~/.ontaria/cache` is still used so older installs keep working.

use std::fs;
#[cfg(feature = "fetch")]
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::parser::RdfFormat;

const EXTENSIONS: &[&str] = &[
    "ttl", "rdf", "xml", "owl", "jsonld", "json", "nt", "nq", "trig",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheMetadata {
    pub url: String,
    pub download_time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

/// Limits applied to every network fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchLimits {
    pub timeout: Duration,
    pub max_bytes: usize,
    pub max_redirects: u32,
}

impl Default for FetchLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_bytes: 32 * 1024 * 1024,
            max_redirects: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyReport {
    pub ok: Vec<String>,
    pub mismatched: Vec<String>,
    pub missing_digest: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CacheManager {
    dir: PathBuf,
}

impl CacheManager {
    /// Open the shared user cache, creating it when it is missing.
    pub fn shared() -> Result<Self> {
        Self::open(default_cache_dir()?)
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
        self.find_content(url).is_some()
    }

    pub fn save(&self, url: &str, content: &[u8]) -> Result<CacheMetadata> {
        let format = crate::parser::detect_format(content);
        self.save_with(url, content, format, None, None)
    }

    pub fn save_with(
        &self,
        url: &str,
        content: &[u8],
        format: RdfFormat,
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Result<CacheMetadata> {
        self.remove_content(url)?;
        let content_path = self
            .dir
            .join(format!("{}.{}", hash_url(url), format.extension()));
        fs::write(&content_path, content).map_err(|source| Error::Write {
            path: content_path,
            source,
        })?;
        let meta = CacheMetadata {
            url: url.to_string(),
            download_time: utc_now_rfc3339(),
            etag,
            last_modified,
            sha256: Some(sha256_hex(content)),
            format: Some(format.as_str().to_string()),
            bytes: Some(content.len() as u64),
        };
        self.write_meta(&meta)?;
        Ok(meta)
    }

    pub fn load(&self, url: &str) -> Result<Vec<u8>> {
        let path = self.find_content(url).ok_or_else(|| Error::NotCached {
            iri: url.to_string(),
        })?;
        fs::read(&path).map_err(|source| Error::Read { path, source })
    }

    pub fn metadata(&self, url: &str) -> Result<CacheMetadata> {
        let path = self.meta_path(url);
        let bytes = fs::read(&path).map_err(|source| Error::Read { path, source })?;
        serde_json::from_slice(&bytes)
            .map_err(|err| Error::message(format!("failed to parse metadata for {url}: {err}")))
    }

    /// Download `url` with the default limits.
    pub fn fetch(&self, url: &str) -> Result<CacheMetadata> {
        self.fetch_limited(url, &FetchLimits::default(), false)
    }

    /// Download `url`. When `refresh` is set and the cache has validators,
    /// a 304 response keeps the stored bytes.
    pub fn fetch_limited(
        &self,
        url: &str,
        limits: &FetchLimits,
        refresh: bool,
    ) -> Result<CacheMetadata> {
        #[cfg(not(feature = "fetch"))]
        {
            let _ = (url, limits, refresh);
            Err(Error::feature(
                "fetch",
                "network downloads are disabled; enable the `fetch` feature or use a local file",
            ))
        }
        #[cfg(feature = "fetch")]
        {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(Error::Fetch {
                    url: url.to_string(),
                    message: "only http and https URLs can be fetched".to_string(),
                });
            }
            let previous = if refresh && self.exists(url) {
                self.metadata(url).ok()
            } else {
                None
            };
            let config = ureq::Agent::config_builder()
                .timeout_global(Some(limits.timeout))
                .max_redirects(limits.max_redirects)
                .http_status_as_error(false)
                .build();
            let agent: ureq::Agent = config.into();
            let mut request = agent
                .get(url)
                .header("User-Agent", "ontaria/0.9")
                .header(
                    "Accept",
                    "text/turtle, application/n-triples, application/n-quads, application/trig, application/rdf+xml, application/ld+json, application/json, */*;q=0.1",
                );
            if let Some(previous) = &previous {
                if let Some(etag) = &previous.etag {
                    request = request.header("If-None-Match", etag);
                }
                if let Some(modified) = &previous.last_modified {
                    request = request.header("If-Modified-Since", modified);
                }
            }
            let mut response = request.call().map_err(|err| Error::Fetch {
                url: url.to_string(),
                message: err.to_string(),
            })?;
            let status = response.status().as_u16();
            if status == 304 {
                return self.metadata(url);
            }
            if status != 200 {
                return Err(Error::HttpStatus {
                    url: url.to_string(),
                    status,
                });
            }
            let etag = header_string(&response, "etag");
            let last_modified = header_string(&response, "last-modified");
            let bytes =
                read_limited(response.body_mut().as_reader(), limits.max_bytes).map_err(|err| {
                    match err {
                        LimitRead::Io(err) => Error::Fetch {
                            url: url.to_string(),
                            message: err.to_string(),
                        },
                        LimitRead::TooLarge => Error::Limit {
                            url: url.to_string(),
                            message: format!("response is larger than {} bytes", limits.max_bytes),
                        },
                    }
                })?;
            let format = crate::parser::detect_format(&bytes);
            self.save_with(url, &bytes, format, etag, last_modified)
        }
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

    /// Delete cached ontologies whose download time is older than `max_age`.
    pub fn prune(&self, max_age: Duration) -> Result<Vec<CacheMetadata>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let mut removed = Vec::new();
        for meta in self.list()? {
            let Some(downloaded) = parse_unix_secs(&meta.download_time) else {
                continue;
            };
            if Duration::from_secs(now.saturating_sub(downloaded)) < max_age {
                continue;
            }
            self.remove_content(&meta.url)?;
            let meta_path = self.meta_path(&meta.url);
            if meta_path.is_file() {
                fs::remove_file(&meta_path).map_err(|source| Error::Write {
                    path: meta_path,
                    source,
                })?;
            }
            removed.push(meta);
        }
        Ok(removed)
    }

    /// Recompute each stored digest and compare it with the metadata.
    pub fn verify(&self) -> Result<VerifyReport> {
        let mut report = VerifyReport {
            ok: Vec::new(),
            mismatched: Vec::new(),
            missing_digest: Vec::new(),
        };
        for meta in self.list()? {
            let Some(expected) = &meta.sha256 else {
                report.missing_digest.push(meta.url);
                continue;
            };
            let bytes = self.load(&meta.url)?;
            let actual = sha256_hex(&bytes);
            if actual == *expected {
                report.ok.push(meta.url);
            } else {
                report.mismatched.push(meta.url);
            }
        }
        Ok(report)
    }

    fn write_meta(&self, meta: &CacheMetadata) -> Result<()> {
        let meta_path = self.meta_path(&meta.url);
        let bytes = serde_json::to_vec_pretty(meta)
            .map_err(|err| Error::message(format!("failed to encode cache metadata: {err}")))?;
        fs::write(&meta_path, bytes).map_err(|source| Error::Write {
            path: meta_path,
            source,
        })
    }

    fn find_content(&self, url: &str) -> Option<PathBuf> {
        let hash = hash_url(url);
        for extension in EXTENSIONS {
            let path = self.dir.join(format!("{hash}.{extension}"));
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }

    fn remove_content(&self, url: &str) -> Result<()> {
        let hash = hash_url(url);
        for extension in EXTENSIONS {
            let path = self.dir.join(format!("{hash}.{extension}"));
            if path.is_file() {
                fs::remove_file(&path).map_err(|source| Error::Write {
                    path: path.clone(),
                    source,
                })?;
            }
        }
        Ok(())
    }

    fn meta_path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.meta.json", hash_url(url)))
    }
}

pub fn default_cache_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("ONTARIA_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let legacy = Path::new(&home).join(".ontaria").join("cache");
        if legacy.is_dir() {
            return Ok(legacy);
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(xdg).join("ontaria"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(Path::new(&home).join(".cache").join("ontaria"));
    }
    Err(Error::message(
        "HOME is not set, so the ontology cache path cannot be built",
    ))
}

fn utc_now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format_unix_secs(secs)
}

/// Format Unix seconds as `YYYY-MM-DDTHH:MM:SSZ`.
fn format_unix_secs(secs: u64) -> String {
    let days = secs / 86_400;
    let tod = secs % 86_400;
    let hour = tod / 3_600;
    let minute = (tod % 3_600) / 60;
    let second = tod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn parse_unix_secs(text: &str) -> Option<u64> {
    let text = text.trim().trim_end_matches('Z');
    let (date, time) = text.split_once('T')?;
    let mut dates = date.split('-');
    let year: i32 = dates.next()?.parse().ok()?;
    let month: u32 = dates.next()?.parse().ok()?;
    let day: u32 = dates.next()?.parse().ok()?;
    if dates.next().is_some() {
        return None;
    }
    let mut times = time.split(':');
    let hour: u64 = times.next()?.parse().ok()?;
    let minute: u64 = times.next()?.parse().ok()?;
    let second: u64 = times.next()?.parse().ok()?;
    if times.next().is_some()
        || hour > 23
        || minute > 59
        || second > 60
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
    {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i32, month: u32, day: u32) -> Option<u64> {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = (year - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    let days = era as i64 * 146_097 + doe as i64 - 719_468;
    u64::try_from(days).ok()
}

/// Howard Hinnant's `civil_from_days`. `days` is the count since 1970-01-01.
fn civil_from_days(days: u64) -> (i32, u32, u32) {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash_url(url: &str) -> String {
    sha256_hex(url.as_bytes())
}

#[cfg(feature = "fetch")]
enum LimitRead {
    Io(std::io::Error),
    TooLarge,
}

#[cfg(feature = "fetch")]
fn read_limited(
    mut reader: impl Read,
    max_bytes: usize,
) -> std::result::Result<Vec<u8>, LimitRead> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = reader.read(&mut chunk).map_err(LimitRead::Io)?;
        if read == 0 {
            break;
        }
        if buf.len().saturating_add(read) > max_bytes {
            return Err(LimitRead::TooLarge);
        }
        buf.extend_from_slice(&chunk[..read]);
    }
    Ok(buf)
}

#[cfg(feature = "fetch")]
fn header_string(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_uses_the_url_hash_as_the_file_name() {
        let dir = std::env::temp_dir().join(format!("ontaria-cache-{}", std::process::id()));
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
        assert!(cache.metadata(url).unwrap().sha256.is_some());
        assert_eq!(cache.list().unwrap().len(), 1);
        assert!(
            cache
                .find_content(url)
                .unwrap()
                .extension()
                .is_some_and(|ext| ext == "ttl")
        );
        let report = cache.verify().unwrap();
        assert_eq!(report.ok, vec![url.to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn jsonld_bytes_use_a_jsonld_extension() {
        let dir = std::env::temp_dir().join(format!("ontaria-cache-json-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = CacheManager::open(&dir).unwrap();
        let url = "https://example.org/demo.jsonld";
        cache
            .save(url, br#"{"@id":"http://example.org/a"}"#)
            .unwrap();
        assert!(
            cache
                .find_content(url)
                .unwrap()
                .extension()
                .is_some_and(|ext| ext == "jsonld")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn formats_the_unix_epoch_and_a_known_instant() {
        assert_eq!(format_unix_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_secs(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(parse_unix_secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_unix_secs("2023-11-14T22:13:20Z"), Some(1_700_000_000));
    }
}
