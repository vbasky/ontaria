# ontaria

![ontaria — look up any term in an ontology](docs/banner.png)

Ontaria parses an RDF Schema or OWL ontology and looks up, validates, diffs, and generates from it. The `ontaria` crate is a library. The `ontaria` binary is the `ontaria-cli` crate.

Version 0.9.0 adds Story Object Model checks and EBUCore instance documents. The public API is not frozen. See [STABILITY.md](STABILITY.md) and [CHANGELOG.md](CHANGELOG.md).

## Install

```bash
cargo install --path crates/ontaria-cli
# or, without a checkout:
# cargo install --git https://github.com/vbasky/ontaria ontaria-cli
# binary: ontaria
```

Pushing a `v*` tag builds those binaries for macOS, Linux, and Windows, attaches them to a GitHub release, and publishes `ontaria-macros` and `ontaria` to crates.io.

The library:

```toml
ontaria = "0.9"
```

`fetch` is on by default. Disable it for a build with no HTTP client:

```bash
cargo check -p ontaria --no-default-features
```

Optional features are `async-fetch`, `shacl`, `reasoning`, and `codegen`. The CLI feature `mcp` builds `ontaria mcp`. The library's minimum supported Rust version is 1.88. The `ontaria` binary needs 1.89, which is what the bundled Story Object Model crate requires. CI builds with 1.89.

## Library

`Ontology::load` reads the file, resolves imports, and returns an immutable `Send + Sync` value. Later queries take `&self` and do not touch the network. `parse_document` and `validate_rdf` take the bytes plus a base IRI. The base may be an absolute IRI or a filesystem path. A path is stored as a `file://` IRI.

`validate_rdf` reads Turtle, N-Triples, N-Quads, TriG, RDF/XML, and JSON-LD. It also reads EBUCore XML and MediaInfo JSON, which are not RDF, and keeps their text and `#value` fields. `validate_json` checks a `serde_json::Value` and does not expand `@context`. `is_jsonld_document` and `is_ebucore_instance` tell those cases apart.

```rust
use ontaria::{LoadOptions, Ontology};

let ontology = Ontology::load("testdata/simple.ttl", &LoadOptions {
    allow_network: false,
    ..LoadOptions::default()
})?;
assert_eq!(ontology.get_class("Person").unwrap().label, "Person");
```

## Cache

The cache directory is the first of `--cache-dir`, `ONTARIA_CACHE_DIR`, `~/.ontaria/cache` when that directory already exists, `$XDG_CACHE_HOME/ontaria`, and `~/.cache/ontaria`.

Each entry is `{sha256}.{ext}` plus `{sha256}.meta.json`. `fetch` stores a URL or a local file. Query commands never download. A missing remote ontology fails with a message that contains `not cached`.

`--catalog` is a TOML file of `[imports]` or a minimal OASIS XML catalog. A remote ontology cannot import a `file:` URL unless the catalog maps that IRI. `--lock` writes `ontaria.lock.json`. `--locked` exits 1 when a cached file does not match the lock entry.

## Commands

```bash
ontaria <PATH>...
ontaria <PATH> --log-file <FILE>
ontaria <PATH> get-class <TERM>

ontaria fetch <URL_OR_PATH> [--refresh]
ontaria cache list|prune --older-than <DURATION>|verify

ontaria --ontology <URL_OR_PATH> [--imports] get-class <TERM> [--inherited]
ontaria --ontology <URL_OR_PATH> [--imports] get-property <TERM> [--usage]
ontaria --ontology <URL_OR_PATH> [--imports] get-parents <TERM>
ontaria --ontology <URL_OR_PATH> [--imports] get-children <TERM>
ontaria --ontology <URL_OR_PATH> [--imports] list-class-properties <TERM>
ontaria --ontology <URL_OR_PATH> list-namespaces
ontaria --ontology <URL_OR_PATH> list-imports
ontaria --ontology <URL_OR_PATH> [--imports] resources
ontaria --ontology <URL_OR_PATH> [--imports] search <QUERY> [--kind class|property]
ontaria --ontology <URL_OR_PATH> [--imports] batch-get-class <TERM>...
ontaria --ontology <URL_OR_PATH> [--imports] batch-get-property <TERM>...
ontaria --ontology <URL_OR_PATH> [--imports] batch-search <QUERY>...

ontaria --ontology <URL_OR_PATH> validate <DATA> [--shacl SHAPES] [--turtle]
ontaria --ontology <URL_OR_PATH> lint [--deny warnings]
ontaria diff <BEFORE> <AFTER> [--fail-on breaking]
ontaria --ontology <URL_OR_PATH> gen context|rust|typescript|go|python|json-schema|shacl|pg|docs [--out FILE]
ontaria map validate <MAPPING>
ontaria map apply <MAPPING> <INPUT>
ontaria map reverse <MAPPING> <INPUT>
ontaria som validate <URL_OR_PATH>
ontaria som decode <URL_OR_PATH>
ontaria --ontology <URL_OR_PATH> reason [--owl-rl]
ontaria --ontology <URL_OR_PATH> serve [--bind 127.0.0.1:8080]
ontaria --ontology <URL_OR_PATH> mcp
```

`-o` is `--ontology`. A single positional path names that same ontology. A term may be an IRI, `prefix:Local`, a local name, or a label. `--lang` is a comma-separated preference list. The default label is language-less, then English, then the first value.

With no path and no command, `ontaria` prints help and exits 0. A path with no command loads the file and prints a summary. Quote a glob so the shell does not expand it. `--log-file FILE` writes the JSON envelope to that file. The report is JSON.

Ontology flags apply to lookup, `validate`, `lint`, `reason`, `gen`, `map validate`, `serve`, and `mcp`. `--cache-dir` and `--offline` also apply to `fetch`, `cache`, and `som`. `diff` does not take an ontology. `list-cached` is a hidden alias of `cache list`.

Every success response is JSON:

```json
{ "status": "ok", "data": {}, "warnings": [] }
```

A handled failure is `{"status":"error","message":"..."}` and the process exits 1. `validate` also exits 1 when the report contains errors. `lint --deny warnings` exits 1 on warnings. `diff --fail-on breaking` exits 1 when the suggested bump is major.

## What the parser keeps

Turtle, RDF/XML, JSON-LD, N-Triples, N-Quads, and TriG. Named graphs are merged and reported as a warning.

- classes, datatype, object, and annotation properties, and `rdf:Property`
- labels, comments, and descriptions, with every language tag
- `rdfs:domain` and `rdfs:range`, and schema.org `domainIncludes` and `rangeIncludes`
- subclass, subproperty, equivalent class and property, disjointness, boolean class expressions, and `owl:oneOf`
- property characteristics and OWL restrictions, including qualified cardinality
- named individuals, SKOS concepts and schemes, and ontology metadata
- declared prefixes, plus the built-in `owl`, `rdf`, `rdfs`, `xsd`, `dcterms`, `skos`, `sh`, `prov`, and `schema` prefixes

The schema.org JSON-LD context is embedded as `@vocab` `https://schema.org/`. Terms expand to that prefix. Dates and URL strings stay literals, which matches the official context: it does not coerce `@type`. Other remote contexts need the `fetch` feature. EBUCore XML (`urn:ebu:metadata-schema:ebucore`) and MediaInfo JSON (`#value`) are read as triples under the EBUCore ontology IRI. Property names follow the element names. They are not rewritten to MediaInfo RDF names such as `frameWidth`.

## Other crates

- `ontaria-wasm` compiles to a host cdylib and to WASM with wasm-pack. It does not fetch.
- `ontaria-python` exposes `parse` and `class_label` through PyO3. It does not fetch.
- `skills/ontaria/SKILL.md` tells an agent how to call the CLI and MCP tools.
- `fuzz/` is a separate cargo-fuzz workspace. `benches/load.rs` is the criterion bench.

## Licence

MIT. `SECURITY.md` describes fetch limits and how to report a vulnerability.
