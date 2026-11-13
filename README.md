# ontaria

![ontaria — look up any term in an ontology](docs/banner.png)

Command-line tool for querying an RDF Schema or OWL ontology. It parses Turtle, RDF/XML, and JSON-LD, caches the file, and prints JSON for classes, properties, and how they connect.

**Name.** Ontaria is a directory of the classes and properties in an ontology.

## Build

```bash
cargo build --release
# binary: target/release/ontaria
```

Install it onto your `PATH` with:

```bash
cargo install --path .
```

## Cache

Ontologies are stored in `~/.ontaria/cache`:

- `{sha256 of the url}.ttl` is the downloaded file
- `{sha256 of the url}.meta.json` records the URL and download time

`fetch` also accepts a local Turtle path. The cache key is then the `file://` URL of that file. A query can skip the cache and read a file directly:

```bash
ontaria get-class Person -o ./testdata/simple.ttl
```

## Commands

```bash
ontaria fetch <URL_OR_PATH>
ontaria list-cached

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
ontaria --ontology <URL_OR_PATH> [--imports] batch-search <QUERY>... [--kind class|property]
```

`-o` is the short form of `--ontology`. A term may be a full IRI (`https://schema.org/Person`), a prefix (`schema:Person`), a local name (`Person`), or a label (`person`). `--imports` follows `owl:imports` when a command looks up a term or searches. `--kind` keeps one side of a search.

Every response is JSON:

```json
{ "status": "ok", "data": {}, "warnings": [] }
```

`warnings` is omitted when there are none. A handled failure is `{"status":"error","message":"..."}` and the process exits 1.

## Formats

The syntax is detected from the bytes. A file that starts with `{` or `[` is JSON-LD. A file that starts with an XML tag is RDF/XML. Anything else is Turtle. `fetch` asks for all three.

A JSON-LD `@context` of `https://schema.org` uses that vocabulary, so `headline` is `https://schema.org/headline` and `NewsArticle` is `https://schema.org/NewsArticle`. Other `http` and `https` contexts are fetched. `resources` lists the typed things in the file, such as a news article that uses the vocabulary.

`--imports` loads each `owl:imports` URI, in whichever of those syntaxes it was published, and folds it into the ontology being queried. Parents and properties declared in an import are then visible on the importing class. A cycle is reported in `warnings` and is not followed again.

## What the parser keeps

- `owl:Class` and `rdfs:Class`
- `owl:ObjectProperty`, `owl:DatatypeProperty`, `owl:AnnotationProperty`, and `rdf:Property`
- `rdfs:label`, `rdfs:comment`, and `dcterms:description` (a language-less value wins, then English)
- `rdfs:domain` / `rdfs:range`, plus Schema.org `domainIncludes` / `rangeIncludes` on both `http` and `https`
- `rdfs:subClassOf` between named classes, and `owl:imports`
- OWL restrictions: the property on the class, and cardinality such as `1..*` when the restriction states it
- Prefixes declared in the file, plus the built-in `owl`, `rdf`, `rdfs`, `xsd`, `dcterms`, and `schema` prefixes

## Library

The same operations are available as the `ontaria` crate: `parse_document`, `parse_turtle`, `Ontology`, `CacheManager`, and `OntologyManager`.
