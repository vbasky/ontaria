//! EBUCore instance documents that are not RDF.
//!
//! EBUCore XML uses the namespace `urn:ebu:metadata-schema:ebucore`. MediaInfo
//! JSON uses `ebucore:` keys and `#value`, and has no `@context`. Both become
//! triples. That URN maps to
//! `http://www.ebu.ch/metadata/ontologies/ebucore/ebucore#`. Element names are
//! kept. This reader does not apply the hand-written MediaInfo RDF names such
//! as `frameWidth`.

use oxrdf::{BlankNode, Literal, NamedNode, Triple};
use quick_xml::NsReader;
use quick_xml::events::{BytesCData, BytesStart, BytesText, Event};
use quick_xml::name::ResolveResult;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::extract::{InputTriple, build_ontology};
use crate::model::Ontology;

const EBU_XML_NS: &str = "urn:ebu:metadata-schema:ebucore";
const EBU_ONT: &str = "http://www.ebu.ch/metadata/ontologies/ebucore/ebucore#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const DCTERMS: &str = "http://purl.org/dc/terms/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const MEDIAINFO: &str = "https://mediaarea.net#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_VALUE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#value";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";
const FORMAT_XML: &str = "ebucore-xml";
const FORMAT_JSON: &str = "ebucore-json";

pub fn is_ebucore_instance(bytes: &[u8]) -> bool {
    is_ebucore_xml(bytes) || is_mediainfo_json(bytes)
}

pub(crate) fn is_ebucore_xml(bytes: &[u8]) -> bool {
    contains_bytes(bytes, EBU_XML_NS.as_bytes())
        && !contains_bytes(bytes, b"rdf:RDF")
        && !contains_bytes(bytes, b"http://www.w3.org/1999/02/22-rdf-syntax-ns#")
}

pub(crate) fn is_mediainfo_json(bytes: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return false;
    };
    let Value::Object(map) = &value else {
        return false;
    };
    if map.contains_key("@context")
        || map.contains_key("@graph")
        || map.contains_key("@id")
        || map.contains_key("@type")
    {
        return false;
    }
    json_has_ebu_marker(&value)
}

pub(crate) fn parse_xml(bytes: &[u8], base: &str) -> Result<Ontology> {
    let mut reader = NsReader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut triples = Vec::new();
    let mut next_id = 0u64;
    loop {
        let (resolved, event) = reader
            .read_resolved_event_into(&mut buf)
            .map_err(|err| Error::parse(FORMAT_XML, err))?;
        let resolved = own_namespace(&resolved);
        match event {
            Event::Start(start) => {
                let frame = open_frame(&reader, &resolved, &start, &mut next_id, stack.last())?;
                stack.push(frame);
            }
            Event::Empty(start) => {
                let frame = open_frame(&reader, &resolved, &start, &mut next_id, stack.last())?;
                close_frame(frame, &mut stack, &mut triples)?;
            }
            Event::Text(text) => append_text(&mut stack, &text)?,
            Event::CData(text) => append_cdata(&mut stack, &text)?,
            Event::End(_) => {
                let frame = stack.pop().ok_or_else(|| {
                    Error::parse(
                        FORMAT_XML,
                        "EBUCore XML closed an element that was not open",
                    )
                })?;
                close_frame(frame, &mut stack, &mut triples)?;
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if !stack.is_empty() {
        return Err(Error::parse(
            FORMAT_XML,
            "EBUCore XML ended before every element was closed",
        ));
    }
    let prefixes = xml_prefixes(bytes);
    Ok(build_ontology(base, FORMAT_XML, triples, prefixes))
}

pub(crate) fn parse_json(bytes: &[u8], base: &str) -> Result<Ontology> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|err| Error::parse(FORMAT_JSON, err))?;
    let Value::Object(map) = value else {
        return Err(Error::parse(
            FORMAT_JSON,
            "MediaInfo JSON must be an object",
        ));
    };
    let mut sink = Sink::default();
    let roots: Vec<String> = map
        .keys()
        .filter(|key| !key.starts_with('@') && !key.starts_with('#'))
        .cloned()
        .collect();
    if roots.is_empty() {
        return Err(Error::parse(
            FORMAT_JSON,
            "MediaInfo JSON has no ebucore element",
        ));
    }
    for key in roots {
        let iri = expand_name(&key, &mut sink.prefixes)?;
        let subject = sink.blank();
        sink.link_iri(subject.clone(), RDF_TYPE, &iri)?;
        let ns = namespace_base(&iri);
        let child = map
            .get(&key)
            .ok_or_else(|| Error::parse(FORMAT_JSON, format!("missing MediaInfo element {key}")))?;
        absorb(&mut sink, subject, &ns, child)?;
    }
    Ok(build_ontology(
        base,
        FORMAT_JSON,
        sink.triples,
        sink.prefixes,
    ))
}

struct Frame {
    subject: BlankNode,
    iri: String,
    parent: Option<BlankNode>,
    is_root: bool,
    attrs: Vec<(String, String)>,
    lang: Option<String>,
    text: String,
    children: usize,
}

fn open_frame(
    reader: &NsReader<&[u8]>,
    resolved: &OwnedNamespace,
    start: &BytesStart,
    next_id: &mut u64,
    parent: Option<&Frame>,
) -> Result<Frame> {
    let local = String::from_utf8_lossy(start.local_name().into_inner()).into_owned();
    let iri = element_iri(resolved, &local);
    let ns_base = namespace_base(&iri);
    let mut attrs = Vec::new();
    let mut lang = None;
    for attr in start.attributes() {
        let attr = attr.map_err(|err| Error::parse(FORMAT_XML, err))?;
        let raw = attr.key.into_inner();
        if raw == b"xmlns" || raw.starts_with(b"xmlns:") {
            continue;
        }
        let value = attr
            .unescape_value()
            .map_err(|err| Error::parse(FORMAT_XML, err))?
            .into_owned();
        let (resolved_attr, local_attr) = reader.resolve_attribute(attr.key);
        let resolved_attr = own_namespace(&resolved_attr);
        let local_attr = String::from_utf8_lossy(local_attr.into_inner()).into_owned();
        if is_xml_lang(&resolved_attr, &local_attr) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                lang = Some(trimmed.to_string());
            }
            continue;
        }
        if is_xmlns(&resolved_attr) {
            continue;
        }
        let predicate = attribute_iri(&resolved_attr, &local_attr, &ns_base);
        attrs.push((predicate, value));
    }
    Ok(Frame {
        subject: next_blank(next_id),
        iri,
        parent: parent.map(|frame| frame.subject.clone()),
        is_root: parent.is_none(),
        attrs,
        lang,
        text: String::new(),
        children: 0,
    })
}

fn close_frame(frame: Frame, stack: &mut [Frame], triples: &mut Vec<InputTriple>) -> Result<()> {
    let text = frame.text.trim();
    let complex = frame.is_root || !frame.attrs.is_empty() || frame.children > 0;
    if complex {
        if frame.is_root {
            push_iri(triples, frame.subject.clone(), RDF_TYPE, &frame.iri)?;
        }
        for (predicate, value) in &frame.attrs {
            push_literal(triples, frame.subject.clone(), predicate, value, None)?;
        }
        if !text.is_empty() {
            push_literal(
                triples,
                frame.subject.clone(),
                RDF_VALUE,
                text,
                frame.lang.as_deref(),
            )?;
        }
        if let Some(parent) = frame.parent.clone() {
            push_blank(triples, parent, &frame.iri, frame.subject.clone())?;
            note_child(stack);
        }
    } else if !text.is_empty()
        && let Some(parent) = frame.parent.clone()
    {
        push_literal(triples, parent, &frame.iri, text, frame.lang.as_deref())?;
        note_child(stack);
    }
    Ok(())
}

fn note_child(stack: &mut [Frame]) {
    if let Some(parent) = stack.last_mut() {
        parent.children += 1;
    }
}

fn append_text(stack: &mut [Frame], text: &BytesText) -> Result<()> {
    if let Some(frame) = stack.last_mut() {
        let value = text
            .unescape()
            .map_err(|err| Error::parse(FORMAT_XML, err))?;
        frame.text.push_str(&value);
    }
    Ok(())
}

fn append_cdata(stack: &mut [Frame], text: &BytesCData) -> Result<()> {
    if let Some(frame) = stack.last_mut() {
        frame.text.push_str(&String::from_utf8_lossy(text));
    }
    Ok(())
}

enum OwnedNamespace {
    Bound(String),
    Unbound,
    Unknown(String),
}

fn own_namespace(resolved: &ResolveResult) -> OwnedNamespace {
    match resolved {
        ResolveResult::Bound(ns) => {
            OwnedNamespace::Bound(String::from_utf8_lossy(ns.0).into_owned())
        }
        ResolveResult::Unbound => OwnedNamespace::Unbound,
        ResolveResult::Unknown(prefix) => {
            OwnedNamespace::Unknown(String::from_utf8_lossy(prefix).into_owned())
        }
    }
}

fn element_iri(resolved: &OwnedNamespace, local: &str) -> String {
    match resolved {
        OwnedNamespace::Bound(ns) => format!("{}{local}", map_namespace(ns)),
        OwnedNamespace::Unknown(prefix) => format!("urn:ontaria:prefix:{prefix}:{local}"),
        OwnedNamespace::Unbound => format!("urn:ontaria:prefix::{local}"),
    }
}

fn attribute_iri(resolved: &OwnedNamespace, local: &str, element_ns: &str) -> String {
    match resolved {
        OwnedNamespace::Unbound => format!("{element_ns}{local}"),
        OwnedNamespace::Bound(ns) => format!("{}{local}", map_namespace(ns)),
        OwnedNamespace::Unknown(prefix) => format!("urn:ontaria:prefix:{prefix}:{local}"),
    }
}

fn is_xml_lang(resolved: &OwnedNamespace, local: &str) -> bool {
    local == "lang" && matches!(resolved, OwnedNamespace::Bound(ns) if ns == XML_NS)
}

fn is_xmlns(resolved: &OwnedNamespace) -> bool {
    matches!(resolved, OwnedNamespace::Bound(ns) if ns == XMLNS_NS)
}

fn map_namespace(ns: &str) -> String {
    match ns.trim() {
        EBU_XML_NS => EBU_ONT.to_string(),
        "http://www.ebu.ch/metadata/ontologies/ebucore/ebucore"
        | "http://www.ebu.ch/metadata/ontologies/ebucore/ebucore/"
        | EBU_ONT => EBU_ONT.to_string(),
        "https://mediaarea.net" | "https://mediaarea.net/" | MEDIAINFO => MEDIAINFO.to_string(),
        other => {
            if other.ends_with('#') || other.ends_with('/') {
                other.to_string()
            } else if other.starts_with("http://")
                || other.starts_with("https://")
                || other.starts_with("urn:")
            {
                format!("{other}#")
            } else {
                format!("urn:ontaria:ns:{other}#")
            }
        }
    }
}

fn namespace_base(iri: &str) -> String {
    match iri.rfind(['#', '/']) {
        Some(index) => iri[..=index].to_string(),
        None => iri.to_string(),
    }
}

fn xml_prefixes(bytes: &[u8]) -> Vec<(String, String)> {
    crate::parser::xml_prefixes(bytes)
        .into_iter()
        .map(|(prefix, iri)| (prefix, map_namespace(&iri)))
        .collect()
}

#[derive(Default)]
struct Sink {
    triples: Vec<InputTriple>,
    next: u64,
    prefixes: Vec<(String, String)>,
}

impl Sink {
    fn blank(&mut self) -> BlankNode {
        next_blank(&mut self.next)
    }

    fn link_iri(&mut self, subject: BlankNode, predicate: &str, object: &str) -> Result<()> {
        push_iri(&mut self.triples, subject, predicate, object)
    }

    fn link_blank(&mut self, subject: BlankNode, predicate: &str, object: BlankNode) -> Result<()> {
        push_blank(&mut self.triples, subject, predicate, object)
    }

    fn link_literal(&mut self, subject: BlankNode, predicate: &str, value: &str) -> Result<()> {
        push_literal(&mut self.triples, subject, predicate, value, None)
    }
}

fn absorb(sink: &mut Sink, subject: BlankNode, element_ns: &str, value: &Value) -> Result<()> {
    match value {
        Value::Array(items) => {
            for item in items {
                absorb(sink, subject.clone(), element_ns, item)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, child) in map {
                if key == "#value" {
                    sink.link_literal(subject.clone(), RDF_VALUE, &scalar_string(child))?;
                } else if let Some(name) = key.strip_prefix('@') {
                    if name.is_empty() {
                        continue;
                    }
                    let predicate = format!("{element_ns}{name}");
                    sink.link_literal(subject.clone(), &predicate, &scalar_string(child))?;
                } else {
                    let iri = expand_name(key, &mut sink.prefixes)?;
                    let child_ns = namespace_base(&iri);
                    add_values(sink, subject.clone(), &iri, &child_ns, child)?;
                }
            }
            Ok(())
        }
        other => sink.link_literal(subject, RDF_VALUE, &scalar_string(other)),
    }
}

fn add_values(
    sink: &mut Sink,
    parent: BlankNode,
    predicate: &str,
    element_ns: &str,
    value: &Value,
) -> Result<()> {
    match value {
        Value::Array(items) => {
            for item in items {
                add_values(sink, parent.clone(), predicate, element_ns, item)?;
            }
            Ok(())
        }
        Value::Object(map) => add_object_value(sink, parent, predicate, element_ns, map),
        Value::Null => Ok(()),
        other => sink.link_literal(parent, predicate, &scalar_string(other)),
    }
}

fn add_object_value(
    sink: &mut Sink,
    parent: BlankNode,
    predicate: &str,
    element_ns: &str,
    map: &serde_json::Map<String, Value>,
) -> Result<()> {
    let mut attrs = Vec::new();
    let mut children = Vec::new();
    let mut text = None;
    for (key, value) in map {
        if key == "#value" {
            text = Some(scalar_string(value));
        } else if let Some(name) = key.strip_prefix('@') {
            if !name.is_empty() {
                attrs.push((format!("{element_ns}{name}"), scalar_string(value)));
            }
        } else {
            children.push((key.clone(), value));
        }
    }
    if attrs.is_empty() && children.is_empty() {
        if let Some(text) = text {
            sink.link_literal(parent, predicate, &text)?;
        }
        return Ok(());
    }
    let subject = sink.blank();
    sink.link_blank(parent, predicate, subject.clone())?;
    for (iri, value) in &attrs {
        sink.link_literal(subject.clone(), iri, value)?;
    }
    if let Some(text) = text {
        sink.link_literal(subject.clone(), RDF_VALUE, &text)?;
    }
    for (key, value) in children {
        let iri = expand_name(&key, &mut sink.prefixes)?;
        let child_ns = namespace_base(&iri);
        add_values(sink, subject.clone(), &iri, &child_ns, value)?;
    }
    Ok(())
}

fn expand_name(key: &str, prefixes: &mut Vec<(String, String)>) -> Result<String> {
    let Some((prefix, local)) = key.split_once(':') else {
        return Ok(format!("urn:ontaria:prefix::{key}"));
    };
    if local.is_empty() {
        return Err(Error::parse(
            FORMAT_JSON,
            format!("EBUCore name {key} has no local part"),
        ));
    }
    let base = match prefix {
        "ebucore" => EBU_ONT,
        "dc" => DC,
        "dcterms" | "dct" => DCTERMS,
        "rdf" => RDF,
        "rdfs" => RDFS,
        "mediaInfo" => MEDIAINFO,
        "owl" => "http://www.w3.org/2002/07/owl#",
        "xsd" => "http://www.w3.org/2001/XMLSchema#",
        other => return Ok(format!("urn:ontaria:prefix:{other}:{local}")),
    };
    if !prefixes.iter().any(|(name, _)| name == prefix) {
        prefixes.push((prefix.to_string(), base.to_string()));
    }
    Ok(format!("{base}{local}"))
}

fn scalar_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn next_blank(next_id: &mut u64) -> BlankNode {
    let id = format!("b{next_id}");
    *next_id += 1;
    BlankNode::new_unchecked(id)
}

fn named(iri: &str) -> Result<NamedNode> {
    NamedNode::new(iri).map_err(|err| Error::parse("ebucore", format!("invalid IRI {iri}: {err}")))
}

fn push_iri(
    triples: &mut Vec<InputTriple>,
    subject: BlankNode,
    predicate: &str,
    object: &str,
) -> Result<()> {
    triples.push(InputTriple {
        triple: Triple::new(subject, named(predicate)?, named(object)?),
        graph: None,
    });
    Ok(())
}

fn push_blank(
    triples: &mut Vec<InputTriple>,
    subject: BlankNode,
    predicate: &str,
    object: BlankNode,
) -> Result<()> {
    triples.push(InputTriple {
        triple: Triple::new(subject, named(predicate)?, object),
        graph: None,
    });
    Ok(())
}

fn push_literal(
    triples: &mut Vec<InputTriple>,
    subject: BlankNode,
    predicate: &str,
    value: &str,
    language: Option<&str>,
) -> Result<()> {
    triples.push(InputTriple {
        triple: Triple::new(subject, named(predicate)?, literal(value, language)),
        graph: None,
    });
    Ok(())
}

fn literal(value: &str, language: Option<&str>) -> Literal {
    if let Some(language) = language.filter(|language| !language.is_empty())
        && let Ok(literal) = Literal::new_language_tagged_literal(value, language)
    {
        return literal;
    }
    Literal::new_simple_literal(value)
}

fn json_has_ebu_marker(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.keys()
                .any(|key| key.starts_with("ebucore:") || key == "#value")
                || map.values().any(json_has_ebu_marker)
        }
        Value::Array(items) => items.iter().any(json_has_ebu_marker),
        _ => false,
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TermKind;
    use crate::parser::parse_document;

    #[test]
    fn reads_ebucore_xml_title_and_attributes() {
        let xml = br#"<ebucore:ebuCoreMain schema="ebucore.xsd" version="1.8" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:ebucore="urn:ebu:metadata-schema:ebucore">
  <ebucore:coreMetadata>
    <ebucore:title>
      <dc:title>Eurovision Song Contest 2015 Grand Final</dc:title>
    </ebucore:title>
    <ebucore:description typeLabel="Scoreboard Note">
      <dc:description><![CDATA[]]></dc:description>
    </ebucore:description>
    <ebucore:format>
      <ebucore:videoFormat>
        <ebucore:videoEncoding typeLabel="XDCAM HD422 100mbit HD"/>
      </ebucore:videoFormat>
    </ebucore:format>
  </ebucore:coreMetadata>
</ebucore:ebuCoreMain>"#;
        let ontology = parse_document(xml, "http://example.org/ebu").unwrap();
        assert_eq!(ontology.format, "ebucore-xml");
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate == RDF_TYPE && statement.object.ends_with("#ebuCoreMain")
        }));
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate == "http://purl.org/dc/elements/1.1/title"
                && statement.object == "Eurovision Song Contest 2015 Grand Final"
                && statement.kind == TermKind::Literal
        }));
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate.ends_with("#schema") && statement.object == "ebucore.xsd"
        }));
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate.ends_with("#typeLabel")
                && statement.object == "XDCAM HD422 100mbit HD"
        }));
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate.ends_with("#videoEncoding") && statement.kind == TermKind::Blank
        }));
    }

    #[test]
    fn reads_mediainfo_json_values() {
        let json = br##"{
  "ebucore:ebuCoreMain": {
    "@version": "1.8",
    "ebucore:width": [{ "@unit": "pixel", "#value": "1280" }],
    "ebucore:frameRate": [{ "#value": 25 }]
  }
}"##;
        let ontology = parse_document(json, "http://example.org/media").unwrap();
        assert_eq!(ontology.format, "ebucore-json");
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate.ends_with("#width") && statement.kind == TermKind::Blank
        }));
        assert!(
            ontology.statements.iter().any(|statement| {
                statement.predicate == RDF_VALUE && statement.object == "1280"
            })
        );
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate.ends_with("#unit") && statement.object == "pixel"
        }));
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate.ends_with("#frameRate")
                && statement.object == "25"
                && statement.kind == TermKind::Literal
        }));
        assert!(ontology.statements.iter().any(|statement| {
            statement.predicate.ends_with("#version") && statement.object == "1.8"
        }));
    }
}
