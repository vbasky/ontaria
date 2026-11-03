# vocable

![vocable — look up any term in an ontology](docs/banner.png)

Command-line tool for querying an RDF Schema or OWL ontology. It parses Turtle, caches the file, and prints JSON for classes, properties, and how they connect.

**Name.** A vocable is a word as a vocabulary enters it. The tool looks up that word — a class or a property — and returns the entry.

## Build

```bash
cargo build --release
# binary: target/release/vocable
```

Install it onto your `PATH` with:

```bash
cargo install --path .
```

## Cache

Ontologies are stored in `~/.vocable/cache`:

- `{sha256 of the url}.ttl` is the downloaded file
- `{sha256 of the url}.meta.json` records the URL and download time

`fetch` also accepts a local Turtle path. The cache key is then the `file://` URL of that file. A query can skip the cache and read a file directly:

```bash
vocable get-class Person -o ./testdata/simple.ttl
```

## Commands

```bash
vocable fetch <URL_OR_PATH>
vocable list-cached

vocable --ontology <URL_OR_PATH> [--imports] get-class <TERM> [--inherited]
vocable --ontology <URL_OR_PATH> [--imports] get-property <TERM> [--usage]
vocable --ontology <URL_OR_PATH> [--imports] get-parents <TERM>
vocable --ontology <URL_OR_PATH> [--imports] get-children <TERM>
vocable --ontology <URL_OR_PATH> [--imports] list-class-properties <TERM>
vocable --ontology <URL_OR_PATH> list-namespaces
vocable --ontology <URL_OR_PATH> list-imports

vocable --ontology <URL_OR_PATH> [--imports] search <QUERY> [--kind class|property]
vocable --ontology <URL_OR_PATH> [--imports] batch-get-class <TERM>...
vocable --ontology <URL_OR_PATH> [--imports] batch-get-property <TERM>...
vocable --ontology <URL_OR_PATH> [--imports] batch-search <QUERY>... [--kind class|property]
```

`-o` is the short form of `--ontology`. A term may be a full IRI (`https://schema.org/Person`), a prefix (`schema:Person`), a local name (`Person`), or a label (`person`). `--imports` follows `owl:imports` when a command looks up a term or searches. `--kind` keeps one side of a search.

Every response is JSON:

```json
{ "status": "ok", "data": {}, "warnings": [] }
```

`warnings` is omitted when there are none. A handled failure is `{"status":"error","message":"..."}` and the process exits 1.

## What the parser keeps

- `owl:Class` and `rdfs:Class`
- `owl:ObjectProperty`, `owl:DatatypeProperty`, `owl:AnnotationProperty`, and `rdf:Property`
- `rdfs:label`, `rdfs:comment`, and `dcterms:description` (a language-less value wins, then English)
- `rdfs:domain` / `rdfs:range`, plus Schema.org `domainIncludes` / `rangeIncludes` on both `http` and `https`
- `rdfs:subClassOf` between named classes, and `owl:imports`
- OWL restrictions: the property on the class, and cardinality such as `1..*` when the restriction states it
- Prefixes declared in the file, plus the built-in `owl`, `rdf`, `rdfs`, `xsd`, `dcterms`, and `schema` prefixes

`--imports` loads each `owl:imports` URI from the cache, fetching it when it is missing. A cycle is reported in `warnings` and is not followed again.

## Library

The same operations are available as the `vocable` crate: `parse_turtle`, `Ontology`, `CacheManager`, and `OntologyManager`.
