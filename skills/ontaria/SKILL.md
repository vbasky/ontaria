---
name: ontaria
description: Look up, validate, diff, lint, and generate from an RDF Schema or OWL ontology with the ontaria CLI or its MCP server.
---

# Ontaria

Use the `ontaria` binary. It prints one JSON object to stdout. Success is `{"status":"ok","data":...,"warnings":[...]}`. Failure is `{"status":"error","message":"..."}` and the process exits 1. Warnings are objects with `code` and `message`. Logs from `serve` and `mcp` go to stderr.

A term is an IRI, `prefix:Local`, a local name, or a label. Pass the ontology with `-o` / `--ontology`, or as one positional path. Query commands never download. Run `fetch` first for a remote ontology.

`ontaria` with no path and no command prints help and exits 0. `ontaria ./vocab.ttl` loads that file and prints a JSON summary (`format`, `classes`, `properties`, `namespaces`). Several paths, or a quoted glob, return an array. `--log-file FILE` writes the envelope to that file instead of stdout. The report is JSON.

Ontology flags (`-o`, `--imports`, `--catalog`, `--lang`, `--locked`, `--lock`) belong to lookup, `validate`, `lint`, `reason`, `gen`, `map validate`, `serve`, and `mcp`. They are accepted before the command and on that command's help. `--cache-dir` and `--offline` also belong to `fetch`, `cache`, and `som`. `diff`, `map apply`, and `map reverse` do not take an ontology. `cache list` lists the cache. `list-cached` is a hidden alias.

## Lookup

```bash
ontaria fetch https://example.org/vocab.ttl
ontaria -o ./vocab.ttl get-class Person --inherited
ontaria -o ./vocab.ttl get-property name --usage
ontaria -o ./vocab.ttl get-parents Person
ontaria -o ./vocab.ttl get-children Agent
ontaria -o ./vocab.ttl list-class-properties Person
ontaria -o ./vocab.ttl search person --kind class
ontaria -o ./vocab.ttl batch-get-class Person Agent
ontaria -o ./vocab.ttl resources
```

`--imports` follows `owl:imports` from the catalog and the cache. `--offline` is the default for queries. `--lang en,fr` picks a label language. `--cache-dir` and `ONTARIA_CACHE_DIR` choose the cache. `--catalog` maps import IRIs to local files. `--locked` fails when the cache does not match the lock file.

## Checks and generation

```bash
ontaria -o ./vocab.ttl validate ./instance.json
ontaria -o ./vocab.ttl validate ./instance.ttl --shacl ./shapes.ttl
ontaria -o ./vocab.ttl validate ./instance.ttl --turtle
ontaria -o ./vocab.ttl lint --deny warnings
ontaria diff ./before.ttl ./after.ttl --fail-on breaking
ontaria -o ./vocab.ttl gen context --out ./context.jsonld
ontaria -o ./vocab.ttl gen rust --out ./src/terms.rs
ontaria -o ./vocab.ttl gen typescript --out ./terms.ts
ontaria -o ./vocab.ttl gen go --out ./terms.go
ontaria -o ./vocab.ttl gen python --out ./terms.py
ontaria -o ./vocab.ttl gen json-schema --out ./schema.json
ontaria -o ./vocab.ttl gen shacl --out ./shapes.ttl
ontaria -o ./vocab.ttl gen pg --out ./model.pg
ontaria -o ./vocab.ttl gen docs --out ./VOCAB.md
ontaria -o ./vocab.ttl reason
ontaria map validate ./mapping.yml
ontaria som validate ./message.json
ontaria som decode ./message.json
ontaria map apply ./mapping.yml ./record.json
ontaria map reverse ./mapping.yml ./record.jsonld
ontaria fetch https://example.org/vocab.ttl --refresh
ontaria cache verify
```

`som validate` and `som decode` need the CLI built with `--features som`. `cargo install ontaria-cli` leaves that feature off, because the vendored `som` crate is not on crates.io. With the feature, they check a Story Object Model 1.0 message with the upstream `som` crate. The argument is a file path or an `http`/`https` URL. `som validate` exits 1 when the message is rejected. `validate` accepts Turtle, RDF/XML, TriG, N-Triples, JSON-LD, EBUCore XML, and MediaInfo JSON. A filesystem path is a valid base IRI. JSON-LD (`@context` or `@graph`) is expanded before the check. Plain JSON is not. `validate` exits 1 when the report has errors. `lint --deny warnings` exits 1 on warnings. `diff --fail-on breaking` exits 1 when the bump is major. `map validate` exits 1 when a target term is not in the ontology. `validate --shacl` needs the CLI `shacl` feature. `reason --owl-rl` needs the CLI `reasoning` feature. Without that feature, `apply_owl_rl` returns `Error::Feature`. Version 0.9 is pre-1.0. The public API is not frozen. See `STABILITY.md`.

## MCP and HTTP

`ontaria -o ./vocab.ttl mcp` speaks MCP on stdio. Tools are `lookup`, `search`, `hierarchy`, `validate`, and `diff`. Build the CLI with `--features mcp`. Rig and other MCP clients spawn that command. The server negotiates protocol versions 2024-11-05, 2025-03-26, 2025-06-18, and 2025-11-25. It does not serve streamable HTTP. `ontaria serve` is a separate lookup API.

`ontaria serve -o ./vocab.ttl --bind 127.0.0.1:8080` answers GET `/health`, `/class`, `/property`, `/parents`, `/children`, and `/search`.

## Library

Load once, then share the value:

```rust
let ontology = ontaria::Ontology::load(path, &ontaria::LoadOptions {
    allow_network: false,
    ..ontaria::LoadOptions::default()
})?;
let person = ontology.get_class("Person");
```

Turn off `fetch` with `--no-default-features` when the program must not link a HTTP client. `validate_shacl` returns `Error::Feature` unless the library is built with `--features shacl`.
