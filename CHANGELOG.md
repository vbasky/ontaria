# Changelog

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
