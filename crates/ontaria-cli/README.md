# ontaria-cli

![ontaria — look up any term in an ontology](../../docs/banner.png)

Look up, validate, diff, and generate from an RDF Schema or OWL ontology.
The binary is named `ontaria`. The library crate is [`ontaria`](../../README.md) at `crates/ontaria`. From the repository root, `cargo run --bin ontaria` builds it.

## Install

From a checkout of this repository:

```sh
cargo install --path crates/ontaria-cli
```

From Git, without a checkout:

```sh
cargo install --git https://github.com/vbasky/ontaria ontaria-cli
```

The binary needs Rust 1.89. This package is not published to crates.io. It
depends on the Story Object Model crate vendored at `third_party/som`, and
that crate is not published either.

Running `ontaria` with no path and no subcommand prints the help banner and
exits 0.

## Quick start

```sh
# Load a file and print a one-object summary
ontaria vocab.ttl

# The same summary, written to a file
ontaria vocab.ttl --log-file summary.json

# Look up a term. The path is the ontology.
ontaria vocab.ttl get-class Person

# The same lookup, with the ontology flag after the command.
ontaria get-class Person -o vocab.ttl --inherited

# Check a Story Object Model 1.0 message
ontaria som validate message.json
```

A glob is expanded by the binary. Quote it so the shell leaves it alone:

```sh
ontaria 'ontologies/*.ttl'
```

One file returns one JSON object in `data`. Several files return a JSON array.
A pattern that matches nothing exits 1 with `no files matched`.

## Options

| Flag | Where it applies | Description |
| --- | --- | --- |
| `PATH` | Arguments | File, URL, or glob. With no command, each match is summarized. With a command that loads an ontology, one path names that ontology. |
| `-o`, `--ontology` | Ontology commands | The same ontology as one `PATH`. Repeat it only when both names are the same file. |
| `--imports`, `--catalog`, `--lang`, `--locked`, `--lock` | Ontology commands | How that ontology is loaded. |
| `--cache-dir`, `--offline` | Ontology commands, `fetch`, `cache`, `som` | Cache location and network use. |
| `--log-file <FILE>` | Every command | Write the JSON envelope to a file instead of stdout. |
| `-h`, `--help` | Every command | Print help. |
| `-V`, `--version` | The root command | Print version. |

Ontology commands are the Lookup commands, `validate`, `lint`, `reason`, `gen`, `map validate`, `serve`, and `mcp`. `diff`, `map apply`, and `map reverse` compare or rewrite files and do not take an ontology. `fetch`, `cache`, and `som` take the cache flags and do not take `--ontology`.

A flag may come before the command or after the command that accepts it. `ontaria diff --help` lists only `diff`'s own flags.

Every success response is one JSON object:

```json
{ "status": "ok", "data": {}, "warnings": [] }
```

A handled failure is `{"status":"error","message":"..."}` and the process exits 1.

## Subcommands

```sh
ontaria fetch <URL_OR_PATH> [--refresh]
ontaria cache list|prune --older-than <DURATION>|verify

ontaria <PATH> get-class <TERM> [--inherited]
ontaria <PATH> get-property <TERM> [--usage]
ontaria <PATH> get-parents <TERM>
ontaria <PATH> get-children <TERM>
ontaria <PATH> list-class-properties <TERM>
ontaria <PATH> list-namespaces
ontaria <PATH> list-imports
ontaria <PATH> resources
ontaria <PATH> search <QUERY> [--kind class|property]
ontaria <PATH> batch-get-class <TERM>...
ontaria <PATH> batch-get-property <TERM>...
ontaria <PATH> batch-search <QUERY>...

ontaria <PATH> validate <DATA> [--shacl SHAPES] [--turtle]
ontaria <PATH> lint [--deny warnings]
ontaria diff <BEFORE> <AFTER> [--fail-on breaking]
ontaria <PATH> gen context|rust|typescript|go|python|json-schema|shacl|pg|docs [--out FILE]
ontaria map validate <MAPPING>
ontaria map apply <MAPPING> <INPUT>
ontaria map reverse <MAPPING> <INPUT>
ontaria som validate <URL_OR_PATH>
ontaria som decode <URL_OR_PATH>
ontaria <PATH> reason [--owl-rl]
ontaria <PATH> serve [--bind 127.0.0.1:8080]
ontaria <PATH> mcp
```

`mcp` needs `--features mcp`. `validate --shacl` needs `--features shacl`.
`reason --owl-rl` needs `--features reasoning`.

## License

MIT.
