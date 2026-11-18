//! Offline import catalogs.
//!
//! A catalog maps an import IRI to a local file so a build never touches the
//! network. Ontaria reads a TOML file or a small OASIS XML catalog.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub imports: BTreeMap<String, PathBuf>,
    pub base: PathBuf,
}

#[derive(Debug, Deserialize)]
struct TomlCatalog {
    #[serde(default)]
    imports: BTreeMap<String, String>,
}

impl Catalog {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let text = String::from_utf8(bytes)
            .map_err(|err| Error::message(format!("catalog is not UTF-8: {err}")))?;
        let base = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
            || text.contains("<catalog")
        {
            return Ok(Self {
                imports: parse_oasis(&text, &base),
                base,
            });
        }
        let parsed: TomlCatalog = toml::from_str(&text)
            .map_err(|err| Error::message(format!("failed to parse catalog: {err}")))?;
        let mut imports = BTreeMap::new();
        for (iri, relative) in parsed.imports {
            imports.insert(iri, base.join(relative));
        }
        Ok(Self { imports, base })
    }

    pub fn resolve(&self, iri: &str) -> Option<&Path> {
        self.imports.get(iri).map(PathBuf::as_path)
    }
}

/// Read `uri name="..." uri="..."` entries from an OASIS XML catalog.
fn parse_oasis(text: &str, base: &Path) -> BTreeMap<String, PathBuf> {
    let mut imports = BTreeMap::new();
    let mut rest = text;
    while let Some(start) = rest.find("<uri ") {
        rest = &rest[start + 5..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let element = &rest[..end];
        rest = &rest[end..];
        let Some(name) = attribute(element, "name") else {
            continue;
        };
        let Some(uri) = attribute(element, "uri") else {
            continue;
        };
        if uri.starts_with("http://") || uri.starts_with("https://") {
            continue;
        }
        imports.insert(name, base.join(uri));
    }
    imports
}

fn attribute(element: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=");
    let start = element.find(&needle)? + needle.len();
    let rest = element[start..].trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = &rest[quote.len_utf8()..];
    let end = body.find(quote)?;
    Some(body[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_toml_and_oasis_catalogs() {
        let dir = std::env::temp_dir().join(format!("ontaria-catalog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let toml_path = dir.join("catalog.toml");
        fs::write(
            &toml_path,
            "[imports]\n\"http://example.org/other\" = \"other.ttl\"\n",
        )
        .unwrap();
        let catalog = Catalog::load(&toml_path).unwrap();
        assert_eq!(
            catalog.resolve("http://example.org/other"),
            Some(dir.join("other.ttl").as_path())
        );
        let xml_path = dir.join("catalog.xml");
        fs::write(
            &xml_path,
            r#"<catalog xmlns="urn:oasis:names:tc:entity:xmlns:xml:catalog">
                <uri name="http://example.org/owl" uri="owl.rdf"/>
            </catalog>"#,
        )
        .unwrap();
        let catalog = Catalog::load(&xml_path).unwrap();
        assert_eq!(
            catalog.resolve("http://example.org/owl"),
            Some(dir.join("owl.rdf").as_path())
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
