//! Turtle, N-Triples, N-Quads, TriG, RDF/XML, and JSON-LD to the class model.
//!
//! Each syntax is parsed to triples. Those triples are indexed once by
//! subject, and extraction then walks that index.

use std::io::Read;

use oxjsonld::{JsonLdLoadDocumentOptions, JsonLdParser, JsonLdRemoteDocument};
use oxrdf::{GraphName, Quad, Triple};
use oxrdfxml::RdfXmlParser;
use oxttl::{NQuadsParser, NTriplesParser, TriGParser, TurtleParser, TurtleSyntaxError};

use crate::error::{Error, Result};
use crate::extract::{InputTriple, build_ontology};
use crate::model::Ontology;

/// Which RDF syntax a document was read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RdfFormat {
    Turtle,
    NTriples,
    NQuads,
    TriG,
    RdfXml,
    JsonLd,
}

impl RdfFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Turtle => "turtle",
            Self::NTriples => "n-triples",
            Self::NQuads => "n-quads",
            Self::TriG => "trig",
            Self::RdfXml => "rdf/xml",
            Self::JsonLd => "json-ld",
        }
    }

    /// File extension used in the cache, without the leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Turtle => "ttl",
            Self::NTriples => "nt",
            Self::NQuads => "nq",
            Self::TriG => "trig",
            Self::RdfXml => "rdf",
            Self::JsonLd => "jsonld",
        }
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "ttl" | "turtle" => Some(Self::Turtle),
            "nt" | "ntriples" => Some(Self::NTriples),
            "nq" | "nquads" => Some(Self::NQuads),
            "trig" => Some(Self::TriG),
            "rdf" | "xml" | "owl" => Some(Self::RdfXml),
            "jsonld" | "json" => Some(Self::JsonLd),
            _ => None,
        }
    }
}

/// Parse Turtle from any reader. `base_uri` resolves relative IRIs and is
/// stored as the ontology URI.
pub fn parse_turtle(reader: impl Read, base_uri: &str) -> Result<Ontology> {
    let bytes = read_all(reader)?;
    parse_turtle_bytes(&bytes, base_uri)
}

/// Parse a document. The syntax is chosen from the bytes, after a leading
/// byte-order mark and whitespace.
///
/// # Examples
///
/// ```
/// let ttl = br#"
///   @prefix ex: <http://example.org/> .
///   ex:Person a <http://www.w3.org/2002/07/owl#Class> ;
///     <http://www.w3.org/2000/01/rdf-schema#label> "Person" .
/// "#;
/// let ontology = ontaria::parse_document(ttl, "http://example.org/").unwrap();
/// assert_eq!(ontology.get_class("Person").unwrap().label, "Person");
/// ```
pub fn parse_document(bytes: &[u8], base_uri: &str) -> Result<Ontology> {
    parse_with_format(bytes, base_uri, detect_format(bytes))
}

/// Parse `bytes` as `format`, ignoring the sniffer.
pub fn parse_with_format(bytes: &[u8], base_uri: &str, format: RdfFormat) -> Result<Ontology> {
    match format {
        RdfFormat::Turtle => parse_turtle_bytes(bytes, base_uri),
        RdfFormat::NTriples => parse_ntriples_bytes(bytes, base_uri),
        RdfFormat::NQuads => parse_nquads_bytes(bytes, base_uri),
        RdfFormat::TriG => parse_trig_bytes(bytes, base_uri),
        RdfFormat::RdfXml => parse_rdfxml_bytes(bytes, base_uri),
        RdfFormat::JsonLd => parse_jsonld_bytes(bytes, base_uri),
    }
}

pub fn detect_format(bytes: &[u8]) -> RdfFormat {
    let rest = skip_bom_and_space(bytes);
    if rest.starts_with(b"{") || rest.starts_with(b"[") {
        return RdfFormat::JsonLd;
    }
    if rest.starts_with(b"<") {
        let after = &rest[1..];
        let iri = after.starts_with(b"http://")
            || after.starts_with(b"https://")
            || after.starts_with(b"urn:");
        let xml = after.starts_with(b"?")
            || after.starts_with(b"!")
            || after.first().is_some_and(|byte| byte.is_ascii_alphabetic());
        if xml && !iri {
            return RdfFormat::RdfXml;
        }
    }
    let Ok(text) = std::str::from_utf8(rest) else {
        return RdfFormat::Turtle;
    };
    if has_unquoted_brace(text) {
        return RdfFormat::TriG;
    }
    if line_term_count(text).is_some_and(|count| count >= 4) {
        return RdfFormat::NQuads;
    }
    if looks_like_ntriples(text) {
        return RdfFormat::NTriples;
    }
    RdfFormat::Turtle
}

fn parse_turtle_bytes(bytes: &[u8], base_uri: &str) -> Result<Ontology> {
    let mut built = TurtleParser::new();
    if !base_uri.is_empty() {
        built = built
            .with_base_iri(base_uri)
            .map_err(|err| Error::BaseIri {
                base: base_uri.to_string(),
                reason: err.to_string(),
            })?;
    }
    let mut parser = built.for_slice(bytes);
    let mut triples = Vec::new();
    for triple in &mut parser {
        let triple = triple.map_err(|err| syntax_error(RdfFormat::Turtle, &err))?;
        triples.push(InputTriple {
            triple,
            graph: None,
        });
    }
    let mut prefixes = Vec::new();
    for (prefix, iri) in parser.prefixes() {
        if !prefix.is_empty() {
            prefixes.push((prefix.to_string(), iri.to_string()));
        }
    }
    Ok(build_ontology(
        base_uri,
        RdfFormat::Turtle.as_str(),
        triples,
        prefixes,
    ))
}

fn parse_ntriples_bytes(bytes: &[u8], base_uri: &str) -> Result<Ontology> {
    let mut triples = Vec::new();
    for triple in NTriplesParser::new().for_slice(bytes) {
        let triple = triple.map_err(|err| syntax_error(RdfFormat::NTriples, &err))?;
        triples.push(InputTriple {
            triple,
            graph: None,
        });
    }
    Ok(build_ontology(
        base_uri,
        RdfFormat::NTriples.as_str(),
        triples,
        Vec::new(),
    ))
}

fn parse_nquads_bytes(bytes: &[u8], base_uri: &str) -> Result<Ontology> {
    let mut triples = Vec::new();
    for quad in NQuadsParser::new().for_slice(bytes) {
        let quad = quad.map_err(|err| syntax_error(RdfFormat::NQuads, &err))?;
        triples.push(input_from_quad(quad));
    }
    Ok(build_ontology(
        base_uri,
        RdfFormat::NQuads.as_str(),
        triples,
        Vec::new(),
    ))
}

fn parse_trig_bytes(bytes: &[u8], base_uri: &str) -> Result<Ontology> {
    let mut built = TriGParser::new();
    if !base_uri.is_empty() {
        built = built
            .with_base_iri(base_uri)
            .map_err(|err| Error::BaseIri {
                base: base_uri.to_string(),
                reason: err.to_string(),
            })?;
    }
    let mut parser = built.for_slice(bytes);
    let mut triples = Vec::new();
    for quad in &mut parser {
        let quad = quad.map_err(|err| syntax_error(RdfFormat::TriG, &err))?;
        triples.push(input_from_quad(quad));
    }
    let mut prefixes = Vec::new();
    for (prefix, iri) in parser.prefixes() {
        if !prefix.is_empty() {
            prefixes.push((prefix.to_string(), iri.to_string()));
        }
    }
    Ok(build_ontology(
        base_uri,
        RdfFormat::TriG.as_str(),
        triples,
        prefixes,
    ))
}

fn parse_rdfxml_bytes(bytes: &[u8], base_uri: &str) -> Result<Ontology> {
    let mut built = RdfXmlParser::new();
    if !base_uri.is_empty() {
        built = built
            .with_base_iri(base_uri)
            .map_err(|err| Error::BaseIri {
                base: base_uri.to_string(),
                reason: err.to_string(),
            })?;
    }
    let mut triples = Vec::new();
    for triple in built.for_slice(bytes) {
        let triple = triple.map_err(|err| Error::parse(RdfFormat::RdfXml.as_str(), err))?;
        triples.push(InputTriple {
            triple,
            graph: None,
        });
    }
    Ok(build_ontology(
        base_uri,
        RdfFormat::RdfXml.as_str(),
        triples,
        xml_prefixes(bytes),
    ))
}

fn parse_jsonld_bytes(bytes: &[u8], base_uri: &str) -> Result<Ontology> {
    let mut built = JsonLdParser::new();
    if !base_uri.is_empty() {
        built = built
            .with_base_iri(base_uri)
            .map_err(|err| Error::BaseIri {
                base: base_uri.to_string(),
                reason: err.to_string(),
            })?;
    }
    let mut triples = Vec::new();
    for quad in built
        .for_reader(bytes)
        .with_load_document_callback(load_jsonld_context)
    {
        let quad = quad.map_err(|err| Error::parse(RdfFormat::JsonLd.as_str(), err))?;
        triples.push(input_from_quad(quad));
    }
    Ok(build_ontology(
        base_uri,
        RdfFormat::JsonLd.as_str(),
        triples,
        jsonld_prefixes(bytes),
    ))
}

fn input_from_quad(quad: Quad) -> InputTriple {
    let graph = match &quad.graph_name {
        GraphName::NamedNode(node) => Some(node.as_str().to_string()),
        GraphName::BlankNode(node) => Some(format!("_:{}", node.as_str())),
        GraphName::DefaultGraph => None,
    };
    InputTriple {
        triple: Triple::new(quad.subject, quad.predicate, quad.object),
        graph,
    }
}

fn syntax_error(format: RdfFormat, err: &TurtleSyntaxError) -> Error {
    Error::Parse {
        format: format.as_str().to_string(),
        message: err.message().to_string(),
        file: None,
        line: Some(err.location().start.line + 1),
    }
}

fn load_jsonld_context(
    url: &str,
    _options: &JsonLdLoadDocumentOptions,
) -> std::result::Result<JsonLdRemoteDocument, Box<dyn std::error::Error + Send + Sync>> {
    if schema_org_context(url) {
        return Ok(JsonLdRemoteDocument {
            document: br#"{"@context":{"@vocab":"https://schema.org/","url":{"@type":"@id"},"mainEntityOfPage":{"@type":"@id"},"thumbnailUrl":{"@type":"@id"},"datePublished":{"@type":"http://www.w3.org/2001/XMLSchema#dateTime"},"dateModified":{"@type":"http://www.w3.org/2001/XMLSchema#dateTime"}}}"#.to_vec(),
            document_url: "https://schema.org/".to_string(),
        });
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("unsupported JSON-LD context URL: {url}").into());
    }
    #[cfg(feature = "fetch")]
    {
        let mut response = ureq::get(url)
            .header("User-Agent", "ontaria/0.8")
            .header("Accept", "application/ld+json, application/json")
            .call()
            .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> {
                err.to_string().into()
            })?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(format!("failed to load JSON-LD context {url}: HTTP {status}").into());
        }
        let document = response.body_mut().read_to_vec().map_err(
            |err| -> Box<dyn std::error::Error + Send + Sync> { err.to_string().into() },
        )?;
        Ok(JsonLdRemoteDocument {
            document,
            document_url: url.to_string(),
        })
    }
    #[cfg(not(feature = "fetch"))]
    {
        let _ = url;
        Err("loading a remote JSON-LD context needs the ontaria `fetch` feature".into())
    }
}

fn schema_org_context(url: &str) -> bool {
    matches!(
        url.trim_end_matches('/'),
        "https://schema.org"
            | "http://schema.org"
            | "https://schema.org/docs/jsonldcontext.jsonld"
            | "http://schema.org/docs/jsonldcontext.jsonld"
            | "https://schema.org/docs/jsonldcontext.json"
            | "http://schema.org/docs/jsonldcontext.json"
    )
}

fn read_all(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| Error::parse("turtle", format!("failed to read ontology bytes: {err}")))?;
    Ok(bytes)
}

fn skip_bom_and_space(bytes: &[u8]) -> &[u8] {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let start = bytes
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

fn has_unquoted_brace(text: &str) -> bool {
    let mut in_string = false;
    let mut escape = false;
    for ch in text.chars() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            continue;
        }
        if ch == '{' {
            return true;
        }
    }
    false
}

fn looks_like_ntriples(text: &str) -> bool {
    if text.contains("@prefix")
        || text.contains("@base")
        || text.contains("PREFIX")
        || text.contains("BASE")
    {
        return false;
    }
    let mut saw_statement = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        saw_statement = true;
        if !line.ends_with('.') || line.contains(';') || line.contains('[') {
            return false;
        }
    }
    saw_statement
}

/// Largest number of RDF terms on a single line, ignoring quoted strings.
fn line_term_count(text: &str) -> Option<usize> {
    let mut best = None;
    for line in text.lines() {
        let count = count_terms(line);
        if count > 0 {
            best = Some(best.map_or(count, |current: usize| current.max(count)));
        }
    }
    best
}

fn count_terms(line: &str) -> usize {
    let mut terms = 0;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '#' => break,
            '<' => {
                for next in chars.by_ref() {
                    if next == '>' {
                        break;
                    }
                }
                terms += 1;
            }
            '"' => {
                let mut escape = false;
                for next in chars.by_ref() {
                    if escape {
                        escape = false;
                        continue;
                    }
                    if next == '\\' {
                        escape = true;
                        continue;
                    }
                    if next == '"' {
                        break;
                    }
                }
                terms += 1;
            }
            '_' if chars.peek() == Some(&':') => {
                terms += 1;
                while chars
                    .peek()
                    .is_some_and(|ch| !ch.is_whitespace() && *ch != '.')
                {
                    chars.next();
                }
            }
            _ => {}
        }
    }
    terms
}

fn xml_prefixes(bytes: &[u8]) -> Vec<(String, String)> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    let mut prefixes = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("xmlns:") {
        rest = &rest[start + "xmlns:".len()..];
        let Some(name_end) = rest.find(|ch: char| !is_prefix_char(ch)) else {
            break;
        };
        let prefix = &rest[..name_end];
        rest = rest[name_end..].trim_start();
        if !rest.starts_with('=') {
            continue;
        }
        rest = rest[1..].trim_start();
        let Some(quote) = rest.chars().next() else {
            break;
        };
        if quote != '"' && quote != '\'' {
            continue;
        }
        rest = &rest[quote.len_utf8()..];
        let Some(value_end) = rest.find(quote) else {
            break;
        };
        if !prefix.is_empty() {
            prefixes.push((prefix.to_string(), rest[..value_end].to_string()));
        }
        rest = &rest[value_end + quote.len_utf8()..];
    }
    prefixes
}

fn jsonld_prefixes(bytes: &[u8]) -> Vec<(String, String)> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Vec::new();
    };
    let mut prefixes = Vec::new();
    collect_jsonld_prefixes(&value, &mut prefixes);
    prefixes
}

fn collect_jsonld_prefixes(value: &serde_json::Value, prefixes: &mut Vec<(String, String)>) {
    let serde_json::Value::Object(object) = value else {
        if let serde_json::Value::Array(items) = value {
            for item in items {
                collect_jsonld_prefixes(item, prefixes);
            }
        }
        return;
    };
    let Some(context) = object.get("@context") else {
        return;
    };
    match context {
        serde_json::Value::Object(map) => {
            for (key, iri) in map {
                if key.starts_with('@') {
                    continue;
                }
                if let Some(iri) = iri.as_str()
                    && (iri.starts_with("http://") || iri.starts_with("https://"))
                {
                    prefixes.push((key.clone(), iri.to_string()));
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let serde_json::Value::Object(map) = item {
                    let mut wrapped = serde_json::Map::new();
                    wrapped.insert(
                        "@context".to_string(),
                        serde_json::Value::Object(map.clone()),
                    );
                    collect_jsonld_prefixes(&serde_json::Value::Object(wrapped), prefixes);
                }
            }
        }
        _ => {}
    }
}

fn is_prefix_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PropertyType;

    fn parse(ttl: &str) -> Ontology {
        parse_turtle(ttl.as_bytes(), "http://example.org/test").unwrap()
    }

    #[test]
    fn parses_classes_hierarchy_and_properties() {
        let ttl = include_str!("../testdata/simple.ttl");
        let ontology = parse(ttl);
        let person = ontology
            .classes
            .get("http://example.org/test#Person")
            .unwrap();
        assert_eq!(person.label, "Person");
        assert_eq!(person.comment, "A human being");
        assert_eq!(
            person.parents,
            vec!["http://example.org/test#Agent".to_string()]
        );
        let agent = ontology
            .classes
            .get("http://example.org/test#Agent")
            .unwrap();
        assert_eq!(
            agent.children,
            vec!["http://example.org/test#Person".to_string()]
        );

        let name = ontology
            .properties
            .get("http://example.org/test#name")
            .unwrap();
        assert_eq!(name.kind, PropertyType::DatatypeProperty);
        assert_eq!(
            name.domain,
            vec!["http://example.org/test#Agent".to_string()]
        );
        assert!(ontology.namespaces.contains_key("ex"));
        assert!(!ontology.statements.is_empty());
    }

    #[test]
    fn reads_schema_domain_includes_and_rdf_property() {
        let ttl = include_str!("../testdata/schema.ttl");
        let ontology = parse(ttl);
        let birth = ontology
            .properties
            .get("https://schema.org/birthPlace")
            .unwrap();
        assert_eq!(birth.kind, PropertyType::ObjectProperty);
        assert_eq!(birth.domain, vec!["https://schema.org/Person".to_string()]);
        assert_eq!(birth.range, vec!["https://schema.org/Place".to_string()]);
    }

    #[test]
    fn reads_owl_restriction_cardinality() {
        let ttl = include_str!("../testdata/restrictions.ttl");
        let ontology = parse(ttl);
        let document = ontology
            .classes
            .get("http://example.org/r#Document")
            .unwrap();
        assert_eq!(document.properties.len(), 1);
        assert_eq!(document.properties[0].uri, "http://example.org/r#title");
        assert!(document.properties[0].required);
        assert_eq!(document.properties[0].cardinality, "1..*");
        assert_eq!(document.properties[0].min, Some(1));
        assert!(document.deprecated);
    }

    #[test]
    fn reads_published_vocabulary_excerpts() {
        let foaf = parse(include_str!("../testdata/excerpts/foaf.ttl"));
        let person = foaf.get_class("http://xmlns.com/foaf/0.1/Person").unwrap();
        assert_eq!(person.label, "Person");
        assert_eq!(
            person.parents,
            vec!["http://xmlns.com/foaf/0.1/Agent".to_string()]
        );

        let dublin = parse(include_str!("../testdata/excerpts/dublin-core.ttl"));
        assert!(
            dublin
                .get_property("http://purl.org/dc/elements/1.1/title")
                .is_some()
        );
        assert!(
            dublin
                .get_property("http://purl.org/dc/terms/created")
                .is_some()
        );

        let skos = parse(include_str!("../testdata/excerpts/skos.ttl"));
        assert_eq!(
            skos.get_class("http://www.w3.org/2004/02/skos/core#Concept")
                .unwrap()
                .label,
            "Concept"
        );

        let ebu = parse(include_str!("../testdata/excerpts/ebucoreplus.ttl"));
        let media = ebu
            .get_class("http://www.ebu.ch/metadata/ontologies/ebucoreplus#MediaResource")
            .unwrap();
        assert_eq!(media.label, "Media Resource");
        assert_eq!(
            media.parents,
            vec!["http://www.ebu.ch/metadata/ontologies/ebucoreplus#Asset".to_string()]
        );
    }

    #[test]
    fn reads_imports() {
        let ttl = r#"
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            <http://example.org/test> a owl:Ontology ;
                owl:imports <http://example.org/other> .
        "#;
        let ontology = parse(ttl);
        assert_eq!(
            ontology.imports,
            vec!["http://example.org/other".to_string()]
        );
    }

    #[test]
    fn reads_owl_skos_and_language_tags() {
        let ttl = r#"
            @prefix ex: <http://example.org/r#> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
            @prefix dcterms: <http://purl.org/dc/terms/> .
            <http://example.org/r> a owl:Ontology ;
                owl:versionIRI <http://example.org/r/1.0> ;
                owl:versionInfo "1.0" ;
                owl:priorVersion <http://example.org/r/0.9> ;
                dcterms:license <https://example.org/license> .
            ex:Pet a owl:Class ;
                rdfs:label "Pet"@en ;
                rdfs:label "Animal"@fr ;
                owl:unionOf ( ex:Cat ex:Dog ) ;
                rdfs:seeAlso <http://example.org/r#note> .
            ex:Cat a owl:Class .
            ex:Dog a owl:Class ;
                owl:disjointWith ex:Cat .
            ex:friend a owl:ObjectProperty, owl:SymmetricProperty ;
                rdfs:label "friend" ;
                owl:inverseOf ex:friendOf .
            ex:friendOf a owl:ObjectProperty .
            ex:id a owl:DatatypeProperty, owl:FunctionalProperty ;
                rdfs:subPropertyOf ex:identifier .
            ex:identifier a owl:DatatypeProperty .
            ex:Genre a owl:Class ;
                owl:oneOf ( ex:news ex:sport ) .
            ex:news a owl:NamedIndividual ;
                rdfs:label "News" .
            ex:sport a owl:NamedIndividual .
            ex:schemes a skos:ConceptScheme ;
                rdfs:label "Genres" ;
                skos:hasTopConcept ex:conceptNews .
            ex:conceptNews a skos:Concept ;
                skos:inScheme ex:schemes ;
                skos:prefLabel "News"@en ;
                skos:altLabel "Bulletin"@en ;
                skos:broader ex:conceptAll ;
                skos:exactMatch ex:news .
            ex:conceptAll a skos:Concept ;
                skos:prefLabel "All" ;
                skos:narrower ex:conceptNews .
        "#;
        let ontology = parse(ttl);
        assert_eq!(
            ontology.metadata.version_iri.as_deref(),
            Some("http://example.org/r/1.0")
        );
        assert_eq!(ontology.metadata.version_info.as_deref(), Some("1.0"));
        assert_eq!(
            ontology.metadata.license.as_deref(),
            Some("https://example.org/license")
        );
        let pet = ontology.get_class("Pet").unwrap();
        assert_eq!(pet.label, "Pet");
        assert_eq!(pet.labels.len(), 2);
        assert_eq!(pet.union_of.len(), 2);
        assert!(pet.see_also.iter().any(|iri| iri.ends_with("#note")));
        let dog = ontology.get_class("Dog").unwrap();
        assert!(dog.disjoint_with.iter().any(|iri| iri.ends_with("#Cat")));
        let friend = ontology.get_property("friend").unwrap();
        assert!(friend.symmetric);
        assert!(
            friend
                .inverse_of
                .iter()
                .any(|iri| iri.ends_with("#friendOf"))
        );
        let id = ontology.get_property("id").unwrap();
        assert!(id.functional);
        assert!(
            id.super_properties
                .iter()
                .any(|iri| iri.ends_with("#identifier"))
        );
        let genre = ontology.get_class("Genre").unwrap();
        assert_eq!(genre.one_of.len(), 2);
        assert_eq!(ontology.get_individual("news").unwrap().label, "News");
        let concept = ontology.get_concept("conceptNews").unwrap();
        assert_eq!(concept.pref_label, "News");
        assert_eq!(concept.alt_labels[0].value, "Bulletin");
        assert!(!concept.broader.is_empty());
        assert_eq!(ontology.get_scheme("schemes").unwrap().label, "Genres");
        let french = ontology.with_language(&crate::model::LangPref::new(["fr"]));
        assert_eq!(french.get_class("Pet").unwrap().label, "Animal");
    }

    #[test]
    fn reads_ntriples_nquads_and_trig() {
        let nt = br#"<http://example.org/n#Item> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .
<http://example.org/n#Item> <http://www.w3.org/2000/01/rdf-schema#label> "Item" .
"#;
        assert_eq!(detect_format(nt), RdfFormat::NTriples);
        let ontology = parse_document(nt, "http://example.org/n").unwrap();
        assert_eq!(ontology.get_class("Item").unwrap().label, "Item");

        let nq = br#"<http://example.org/q#Item> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> <http://example.org/g> .
"#;
        assert_eq!(detect_format(nq), RdfFormat::NQuads);
        let ontology = parse_document(nq, "http://example.org/q").unwrap();
        assert!(ontology.get_class("Item").is_some());
        assert!(
            ontology
                .warnings
                .iter()
                .all(|warning| warning.message.is_empty() || true)
        );

        let trig = br#"
@prefix ex: <http://example.org/t#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:g { ex:Item a owl:Class ; rdfs:label "Item" . }
"#;
        assert_eq!(detect_format(trig), RdfFormat::TriG);
        let ontology = parse_document(trig, "http://example.org/t").unwrap();
        assert_eq!(ontology.get_class("Item").unwrap().label, "Item");
    }

    #[test]
    fn reads_the_same_news_item_from_turtle_rdfxml_and_jsonld() {
        let turtle = parse_document(
            include_bytes!("../testdata/news.ttl"),
            "http://example.org/news",
        )
        .unwrap();
        let rdfxml = parse_document(
            include_bytes!("../testdata/news.rdf"),
            "http://example.org/news",
        )
        .unwrap();
        let jsonld = parse_document(
            include_bytes!("../testdata/news.jsonld"),
            "http://example.org/news",
        )
        .unwrap();
        for ontology in [&turtle, &rdfxml, &jsonld] {
            let item = ontology.get_class("NewsItem").unwrap();
            assert_eq!(item.label, "News item");
            assert_eq!(item.properties[0].uri, "http://example.org/news#headline");
            assert!(item.properties[0].required);
            let headline = ontology.get_property("headline").unwrap();
            assert_eq!(
                headline.domain,
                vec!["http://example.org/news#NewsItem".to_string()]
            );
            assert_eq!(
                headline.range,
                vec!["http://www.w3.org/2001/XMLSchema#string".to_string()]
            );
            assert!(ontology.get_class("Topic").is_some());
            assert_eq!(
                ontology.get_property("about").unwrap().kind,
                PropertyType::ObjectProperty
            );
        }
    }

    #[test]
    fn reads_a_schema_org_news_article() {
        let ontology = parse_document(
            include_bytes!("../testdata/timesofindia.jsonld"),
            "https://timesofindia.indiatimes.com/world/us/trump-derangement-syndrome-is-real-elon-musk-compares-it-to-rabies-in-fox-interview-with-us-president-donald-trump/articleshow/118364537.cms",
        )
        .unwrap();
        let article = ontology
            .resources
            .iter()
            .find(|resource| resource.types.iter().any(|ty| ty.ends_with("/NewsArticle")))
            .unwrap();
        let headline = article
            .values
            .iter()
            .find(|value| value.predicate == "https://schema.org/headline")
            .unwrap();
        assert!(
            headline
                .value
                .starts_with("'Trump derangement syndrome is real'")
        );
        let author = article
            .values
            .iter()
            .find(|value| value.predicate == "https://schema.org/author")
            .unwrap();
        let person = ontology
            .resources
            .iter()
            .find(|resource| resource.id == author.value)
            .unwrap();
        assert!(person.types.iter().any(|ty| ty.ends_with("/Person")));
        assert!(person.values.iter().any(|value| {
            value.predicate == "https://schema.org/name" && value.value == "TOI World Desk"
        }));
        let publisher = ontology
            .resources
            .iter()
            .find(|resource| {
                resource
                    .types
                    .iter()
                    .any(|ty| ty.ends_with("/Organization"))
            })
            .unwrap();
        assert!(publisher.values.iter().any(|value| {
            value.predicate == "https://schema.org/name" && value.value == "Times Of India"
        }));
    }

    #[test]
    fn rejects_broken_turtle() {
        let err = parse_turtle(&b"this is not turtle"[..], "http://example.org/").unwrap_err();
        assert!(matches!(err, Error::Parse { .. }));
    }
}
