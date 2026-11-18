# Ontaria roadmap: from lookup tool to ontology toolkit

Baseline: ontaria 0.0.1 parses Turtle, RDF/XML and JSON-LD (via oxttl, oxrdfxml, oxjsonld),
follows `owl:imports`, caches downloads, and answers class, property and hierarchy queries as JSON
from a CLI and a library.

## 1. Library foundation (do first)

Everything else builds on a clean, embeddable library.

- **Split the crates:** `ontaria` (library, no CLI dependencies) and `ontaria-cli` (binary).
  Alternatively, put `clap` and printing behind a `cli` feature.
- **Cargo features:**
  - `fetch` (ureq; off for offline and embedded use);
  - `async-fetch`;
  - `cli`, `shacl`, `reasoning`, `mcp`, `codegen`.
- **Load once, then no I/O:**
  - `Ontology::load(...)` resolves all imports up front and returns an immutable,
    `Send + Sync` value to share as `Arc<Ontology>`;
  - all query methods take `&self`. Today `OntologyManager` methods take `&mut self` and fetch
    imports lazily.
- **No printing in the library:** `print_success` and `print_error` move to the CLI.
- **Errors:**
  - `#[non_exhaustive]` error enums with context (term, IRI, file, line);
  - parse warnings returned as data, not strings.
- **Serde everywhere:** `Serialize`/`Deserialize` on all public model types.
- **Stable public API:**
  - stop re-exporting internals;
  - document the semver policy and MSRV;
  - add a changelog;
  - complete docs.rs documentation with examples.

## 2. Ontology model completeness

Parse and expose what real ontologies (EBUCore Plus, schema.org, SKOS vocabularies) use.

- **RDFS:** `rdfs:subPropertyOf` (property hierarchy), `rdfs:seeAlso`, `rdfs:isDefinedBy`.
- **OWL classes:**
  - `owl:equivalentClass`, `owl:disjointWith`;
  - `owl:unionOf`, `owl:intersectionOf`, `owl:complementOf`;
  - `owl:oneOf` (enumerations, e.g. controlled genre lists).
- **OWL properties:**
  - `owl:equivalentProperty`, `owl:inverseOf`;
  - `owl:FunctionalProperty`, `owl:InverseFunctionalProperty`;
  - `owl:TransitiveProperty`, `owl:SymmetricProperty`.
- **OWL restrictions:**
  - `owl:someValuesFrom`, `owl:allValuesFrom`, `owl:hasValue`;
  - qualified cardinality, including the target class.
- **Individuals:** `owl:NamedIndividual` and typed individuals as controlled values.
- **SKOS:**
  - `skos:Concept`, `skos:ConceptScheme`;
  - `prefLabel`/`altLabel`/`hiddenLabel`;
  - `broader`/`narrower`/`related`, `exactMatch`/`closeMatch`.
- **Ontology metadata:**
  - `owl:versionIRI`, `owl:versionInfo`, `owl:priorVersion`;
  - `dcterms:created`/`modified`, licence.
- **Languages:**
  - keep all language-tagged labels and comments;
  - add a `lang` preference parameter instead of "language-less wins, then English".
- **Formats:** add N-Triples, N-Quads and TriG (oxttl supports them).
- **Annotations:** arbitrary annotation properties kept and queryable.

## 3. Loading, caching and reproducibility

- **Offline catalog:** a file that maps import IRIs to local files. Builds and CI then never
  touch the network. Consider the OASIS XML catalog format, or a simple YAML/TOML equivalent.
- **Configurable cache:** `ONTARIA_CACHE_DIR` and `--cache-dir`, falling back to the platform
  cache directory. The cache is fixed at `$HOME/.ontaria/cache` today.
- **Correct cache file names:** use the extension of the actual format. Today every file is `.ttl`.
- **Pinning:**
  - a lock file recording IRI, resolved URL, SHA-256 and fetch time;
  - `--locked` fails on mismatch.
- **Refresh:**
  - HTTP `ETag`/`Last-Modified` revalidation;
  - `fetch --refresh`, `cache prune`, `cache verify`.
- **Network limits:** timeouts, size limits and redirect limits on fetches.

## 4. Validation

- **`validate <data>`** checks instance data against the ontology:
  - unknown types and properties;
  - domain and range, including inherited classes and XSD datatypes;
  - cardinality, from restrictions ontaria already parses;
  - `owl:oneOf` and SKOS controlled values;
  - disjointness;
  - deprecated terms (already parsed).
- **SHACL (`shacl` feature):** integrate an existing validator, such as rudof or
  shacl_validation (MIT/Apache-2.0), rather than writing one.
- **Reports:**
  - JSON with path, term, rule, severity and message;
  - optionally a W3C SHACL `ValidationReport` in Turtle;
  - non-zero exit codes for errors, so it can gate CI.
- **Library API:** validate parsed values (`serde_json::Value`, an `oxrdf` graph), not just files.

## 5. Generation

- **`gen context`:** JSON-LD `@context`, with prefixes plus term definitions (`@id`, `@type`
  for datatype properties, `@container` for multi-valued properties).
- **Rust codegen:**
  - a `build.rs` helper (`ontaria::codegen::generate("cx.ttl", out_dir)`);
  - a proc macro (`ontaria::terms!("ontology/cx.ttl")`) producing typed constants and enums
    for classes and properties.

  A misspelt term then fails at compile time.
- **Other targets, via templates:** TypeScript, Go and Python constants.
- **JSON Schema** per class, for OpenAPI documents and form validation.
- **SHACL shape skeletons** from OWL domain, range and restrictions.
- **Property-graph schema emitters,** for example Omnigraph `.pg`:
  - classes → nodes, datatype properties → fields, object properties → edges,
    cardinality → `@card`;
  - each run reports what could not be translated.
- **Documentation:** Markdown/HTML pages per class and property, with hierarchy diagrams (Mermaid).
- **Deterministic output** for every generator, so diffs stay clean in Git.

## 6. Ontology evolution: diff and lint

- **`diff old.ttl new.ttl`:**
  - lists added, removed and changed terms;
  - classifies each change as breaking or non-breaking (removed term, narrowed range,
    changed cardinality);
  - suggests a semver bump.
- **`lint`:**
  - missing labels or comments;
  - properties without domain or range;
  - `subClassOf` cycles;
  - undefined terms referenced by restrictions;
  - unused prefixes;
  - deprecated terms still referenced;
  - IRIs that don't match the ontology namespace.
- **CI mode:** `ontaria lint --deny warnings` and `ontaria diff --fail-on breaking`.

## 7. Reasoning (optional `reasoning` feature)

- RDFS entailment: inferred types, inherited domains and ranges, property hierarchy.
- OWL 2 RL by integrating `reasonable` (BSD-3), rather than writing a reasoner.
- Query results flag which facts are asserted and which are inferred.
- Avoid LGPL-3.0 dependencies (e.g. horned-owl) unless the licence implications are accepted.

## 8. Mapping and translation (later)

- A declarative mapping format, from source JSON to RDF. Evaluate an RML/YARRRML subset or a
  simpler YAML format.
- `map validate`: every target term exists, and domain and range fit.
- `map apply`: source JSON in, JSON-LD out, with a provenance option (PROV-O).
- Reverse mappings, from RDF to a target JSON shape.

## 9. Interfaces

- **MCP server (`mcp` feature, via rmcp, the official Rust MCP SDK):**
  - `ontaria mcp --ontology <path>` exposes lookup, search, hierarchy, validate and diff as
    MCP tools;
  - agents can then query any ontology with no custom code;
  - this directly serves the "look up ontology terms" use case for agents and skills.
- **Agent skill:** ship a `SKILL.md` describing how to use the CLI or MCP tools.
- **WASM / npm** build for browser and Node tooling.
- **Python bindings (pyo3)**, optional, for data teams.
- **HTTP server (optional):** a read-only REST API over a loaded ontology.

## 10. Quality and trust

- Test fixtures from real ontologies: EBUCore Plus, schema.org, SKOS, FOAF, Dublin Core.
- Regression tests for every query and generator output (snapshot tests).
- Fuzzing for parsers and term resolution (cargo-fuzz).
- Benchmarks for load time and query latency on large ontologies (criterion).
- CI: fmt, clippy `-D warnings`, tests, `cargo deny` (licences and advisories), MSRV check.
- Security: size and time limits on fetches; no network access unless the `fetch` feature is on.
- Releases: changelog, semver, signed tags, and a documented stability policy for 1.0.

## Suggested releases

| Version | Theme | Key items |
|---|---|---|
| 0.1 | Embeddable | Crate split, features, preload + `Arc<Ontology>`, no library printing, offline catalog, cache dir |
| 0.2 | Complete model | RDFS/OWL/SKOS features, languages, more formats, ontology metadata |
| 0.3 | Validate + generate | `validate`, `gen context`, Rust codegen (build.rs + macro) |
| 0.4 | Evolution | `diff`, `lint`, CI modes, pinning lock file |
| 0.5 | Agents | MCP server, SKILL.md, SHACL feature |
| 0.6 | Interop | JSON Schema, SHACL skeletons, property-graph emitters, docs generation |
| 0.7 | Reasoning | RDFS entailment, OWL 2 RL via `reasonable` |
| 0.8 | Mapping | Mapping format, `map validate`, `map apply` |
| 1.0 | Stable | Frozen public API, full docs, benchmarks, security review |
