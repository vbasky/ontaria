# Changelog

## Unreleased

- `ontaria-cli` lives at `crates/ontaria-cli`. The binary crate has `src/cli.rs`, `src/banner.txt`, and its own README, and it inherits edition, license, and repository from the workspace.
- With no path and no subcommand, `ontaria` prints help and exits 0.
- Help splits arguments into Input, Output, and Ontology. A positional `PATH` summarizes each file when no command is given. A glob expands to matching files.
- Ontology flags (`-o`, `--imports`, `--catalog`, `--lang`, `--locked`, `--lock`) belong to the commands that load an ontology. `--cache-dir` and `--offline` also belong to `fetch`, `cache`, and `som`. `diff`, `map apply`, and `map reverse` do not take them. `--log-file` is the output flag for every command.
- One `PATH` and `-o` name the same ontology. When both are present they must name the same file. `list-cached` remains available and is hidden from help; `cache list` is the listed command.

## 0.9.0

- `ontaria som validate` and `ontaria som decode` call the upstream Story Object Model 1.0 crate, vendored at `third_party/som` from `storyobjectmodel/som` `ff1e94a`. The argument is a local file or an `http`/`https` URL. The crate's own schemas, examples, and tests are unchanged.
- The `ontaria` binary needs Rust 1.89. CI builds with 1.89. The library `rust-version` stays 1.88.
- A filesystem path is a valid base IRI. `parse_document` and `validate` store it as a `file://` IRI, so Turtle, TriG, RDF/XML, and JSON-LD parse from other crates and from the CLI.
- The embedded schema.org context is `@vocab` `https://schema.org/` only. Dates and URL strings stay literals. `wordcount` is not rewritten to `wordCount`.
- RDF/XML with `xml:lang=""` keeps the literal and drops the empty language tag.
- EBUCore XML and MediaInfo JSON parse to triples. Text and `#value` are kept. The XML namespace `urn:ebu:metadata-schema:ebucore` maps to the EBUCore ontology IRI. Element names are not rewritten to the MediaInfo RDF names.
- `validate` and the MCP `validate` tool expand JSON-LD (`@context` or `@graph`) before checking.

## 0.8.0

- A YAML mapping names a subject, a type, and source paths. `map validate` checks those terms. `map apply` writes JSON-LD, including an optional PROV-O derivation. `map reverse` reads that JSON-LD back into the source shape.
- `fetch --refresh` revalidates with ETag and Last-Modified. `cache list`, `cache prune --older-than`, and `cache verify` maintain the cache.
- `async-fetch` downloads with `reqwest`. The caller supplies the runtime.
- `STABILITY.md` says the public API is not frozen. 1.0 is still the freeze and is not tagged.
- `SECURITY.md` records fetch limits. The client sends `User-Agent: ontaria/0.8`.
- CI runs fmt, clippy, tests, and `cargo deny`. `benches/load.rs` times a parse. `fuzz/` is a separate workspace for the parser.

## 0.7.0

- `reason` materializes RDFS subclass, subproperty, domain, and range consequences. Inferred triples are marked `inferred: true`. Asserted triples stay as they were written.
- `reason --owl-rl` runs OWL 2 RL through `reasonable` when the `reasoning` feature is on. The reasoner is not stored on `Ontology`. Without the feature, `apply_owl_rl` returns `Error::Feature`.

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
