---
name: ontaria
description: Look up, validate, diff, lint, and generate from an RDF Schema or OWL ontology with the ontaria CLI or its MCP server.
---

# Ontaria

Use the `ontaria` binary. It prints one JSON object to stdout. Success is `{"status":"ok","data":...,"warnings":[...]}`. Failure is `{"status":"error","message":"..."}` and the process exits 1. Warnings are objects with `code` and `message`. Logs from `serve` and `mcp` go to stderr.

A term is an IRI, `prefix:Local`, a local name, or a label. Pass the ontology with `-o` or `--ontology`. Query commands never download. Run `fetch` first for a remote ontology.

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
```

`validate` exits 1 when the report has errors. `lint --deny warnings` exits 1 on warnings. `diff --fail-on breaking` exits 1 when the bump is major. `validate --shacl` needs the CLI `shacl` feature.

## MCP and HTTP

`ontaria mcp -o ./vocab.ttl` speaks MCP on stdio. Tools are `lookup`, `search`, `hierarchy`, `validate`, and `diff`. Build the CLI with `--features mcp`.

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
