# Security

Ontaria reads ontologies and instance data that you point it at. Treat those documents as untrusted input.

## Network

The library does not contact the network unless the `fetch` or `async-fetch` feature is enabled and the caller asks for a download. Building with `--no-default-features` removes `ureq`.

CLI query commands load with `allow_network: false`. Only `fetch` and `fetch --refresh` download. A missing cache entry fails with a message that contains `not cached`.

A remote ontology cannot import a `file:` URL. Point that IRI at a local file with a catalog entry instead. Top-level paths you pass on the command line are read directly.

## Fetch limits

Downloads use these defaults:

- timeout: 30 seconds
- maximum body size: 32 MiB
- maximum redirects: 5
- `User-Agent`: `ontaria/0.8`

A response over the size limit is `Error::Limit`. The HTTP server binds to `127.0.0.1:8080` unless you pass `--bind`.

## Cache and lock file

Cached bytes live in the directory from `--cache-dir`, `ONTARIA_CACHE_DIR`, the legacy `~/.ontaria/cache` directory when it already exists, or `$XDG_CACHE_HOME/ontaria` (`~/.cache/ontaria`). `--locked` refuses to use bytes whose SHA-256 does not match the lock file.

## Known dependency notes

`cargo deny` allows the licences in `deny.toml`, including `CDLA-Permissive-2.0` for the CA certificates that `webpki-roots` ships with `ureq`.

Two advisories are ignored because the crates we depend on have not published a fix:

- `serde_yaml` 0.9, used for mapping files, depends on the archived `unsafe-libyaml` crate (`RUSTSEC-2024-0449`).
- `oxrdfxml` 0.2.5 parses RDF/XML with `quick-xml` 0.37 (`RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`). Downloaded documents are rejected above 32 MiB. A local file is only read when you pass its path.

## Reporting

Report a vulnerability through the GitHub Security Advisories page for [vbasky/ontaria](https://github.com/vbasky/ontaria/security/advisories).
