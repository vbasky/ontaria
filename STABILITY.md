# Stability

Ontaria 0.9.0 is a pre-1.0 release. The suggested 1.0 row in `ROADMAP.md` is a frozen public API. That freeze has not happened.

## Versions

- Before 1.0, a new minor version may change the public Rust API, the CLI flags, and the JSON shapes.
- Patch releases keep the public API of that minor version.
- 1.0 will freeze the public API of the `ontaria` library: load, the model types, query, validate, lint, diff, mapping, and the generator text formats.
- `Error` and `WarningCode` are `#[non_exhaustive]`. New variants can arrive in a minor release before 1.0, and in a minor release after 1.0 only when they do not change existing match behavior that uses a wildcard.

## Supported surface

The supported library entry point is `Ontology::load`. The returned value is immutable and `Send + Sync`. Query methods do not perform I/O.

`OntologyManager` is crate-private. Do not depend on it.

The CLI JSON envelope is part of the tool contract for this minor version: success is `{status, data, warnings}` and failure is `{status, message}`. Warnings are objects with a `code` and a `message`.

## Rust

The `ontaria` library's minimum supported Rust version is 1.88. That is the version in its `rust-version`. The crate uses edition 2024 and let chains, which stabilized in 1.88. The `ontaria` binary and the vendored `som` crate need Rust 1.89, and CI builds with 1.89.

## Features

`fetch` is on by default. `async-fetch`, `shacl`, `reasoning`, `codegen`, and the CLI `mcp` feature are optional. Calling a feature's API without the feature returns `Error::Feature`.
