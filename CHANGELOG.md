# Changelog

## 0.6.0

- `gen typescript`, `gen go`, and `gen python` emit language bindings. `gen json-schema`, `gen shacl`, `gen pg`, and `gen docs` emit JSON Schema, SHACL shape skeletons, a property-graph text file, and Markdown with a Mermaid diagram.
- `ontaria-wasm` is a `cdylib` for browser and Node. `ontaria-python` is a pyo3 module. Both load documents the caller passes and do not fetch the network.

## 0.5.0

- `validate --shacl` checks RDF instance data against SHACL shapes. The `shacl` feature uses `shacl-rust`. Without it, `validate_shacl` returns `Error::Feature`.
- `validate --turtle` prints a W3C SHACL ValidationReport.
- `ontaria mcp` exposes lookup, search, hierarchy, validate, and diff on stdio. Build the CLI with `--features mcp`. Logs go to stderr.
- `ontaria serve --bind` is a read-only HTTP API. The default bind is `127.0.0.1:8080`.
- `skills/ontaria/SKILL.md` describes the CLI and the MCP tools.

## 0.4.0

- `diff` classifies added, removed, and changed terms and suggests a semver bump. `--fail-on breaking` exits 1 when the bump is major.
- `lint` reports missing labels, missing domain or range, cycles, undefined restriction terms, unused prefixes, deprecated terms that are still referenced, and IRIs outside the ontology namespace. `--deny warnings` exits 1 on warnings.
- `--lock` writes a lock file of IRI, URL, SHA-256, and fetch time. `--locked` fails when the cached bytes do not match.

## 0.3.0

- `validate` checks instance data and exits 1 when the report has errors.
- `gen context` writes a JSON-LD `@context`. `gen rust` writes Rust constants. `ontaria::codegen::generate` is the `build.rs` helper.
- The `codegen` feature re-exports `terms!` from `ontaria-macros`.

## 0.2.0

- The model keeps RDFS, OWL, and SKOS terms: equivalents, disjointness, boolean class expressions, property characteristics, qualified restrictions, individuals, concepts, and ontology metadata.
- Every language tag is kept. `--lang` selects a preference. The default is language-less, then English, then the first value.
- N-Triples, N-Quads, and TriG parse alongside Turtle, RDF/XML, and JSON-LD. Named graphs are merged and reported as a warning.
- Warnings are objects with a code and a message.

## 0.1.0

- The `ontaria` crate is a library. The `ontaria` binary lives in `ontaria-cli` and is the only crate that prints.
- `Ontology::load` resolves imports once and returns an immutable `Send + Sync` value.
- `fetch` is the default feature. `--no-default-features` does not link `ureq`.
- An offline catalog maps import IRIs to local files. The cache directory is `--cache-dir`, `ONTARIA_CACHE_DIR`, an existing `~/.ontaria/cache`, or the platform cache.
- A remote ontology cannot import a `file:` URL unless the catalog maps that IRI. Query commands do not download.
