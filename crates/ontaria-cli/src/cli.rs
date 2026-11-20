use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

const BANNER: &str = include_str!("banner.txt");

#[derive(Parser)]
#[command(
    name = "ontaria",
    version,
    about = None,
    long_about = None,
    before_help = BANNER,
    subcommand_precedence_over_arg = true
)]
pub(crate) struct Cli {
    /// Ontology file, URL, or glob. Quote a glob so the shell does not expand it.
    /// With no command, each path is summarized. With a command that loads an
    /// ontology, one path names that ontology.
    #[arg(value_name = "PATH", help_heading = "Input")]
    pub paths: Vec<String>,

    /// Write the JSON envelope to a file instead of stdout.
    #[arg(long, global = true, help_heading = "Output", value_name = "FILE")]
    pub log_file: Option<PathBuf>,

    /// Accepted before the command. Commands that load an ontology also accept
    /// these flags after the command name.
    #[command(flatten)]
    pub load: OntologyOpts,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Clone, Default)]
pub(crate) struct LoadFlags {
    pub ontology: Option<String>,
    pub imports: bool,
    pub cache_dir: Option<PathBuf>,
    pub catalog: Option<PathBuf>,
    pub lang: Option<String>,
    pub locked: bool,
    pub lock: Option<PathBuf>,
    pub offline: bool,
}

impl LoadFlags {
    pub(crate) fn ontology_only(&self) -> bool {
        self.ontology.is_some()
            || self.imports
            || self.catalog.is_some()
            || self.lang.is_some()
            || self.locked
            || self.lock.is_some()
    }

    pub(crate) fn any(&self) -> bool {
        self.ontology_only() || self.cache_dir.is_some() || self.offline
    }
}

macro_rules! ontology_opts {
    ($name:ident, $global:literal) => {
        #[derive(Args, Clone, Default)]
        #[command(next_help_heading = "Ontology")]
        pub(crate) struct $name {
            /// Ontology URL, or a path to a local file.
            /// One PATH before the command names this same ontology.
            #[arg(short = 'o', long, global = $global, value_name = "URL_OR_PATH")]
            pub ontology: Option<String>,

            /// Follow owl:imports while loading.
            #[arg(long, global = $global)]
            pub imports: bool,

            /// Cache directory. Overrides ONTARIA_CACHE_DIR.
            #[arg(long, global = $global, value_name = "DIR")]
            pub cache_dir: Option<PathBuf>,

            /// Offline catalog (TOML or OASIS XML) mapping import IRIs to local files.
            #[arg(long, global = $global, value_name = "FILE")]
            pub catalog: Option<PathBuf>,

            /// Preferred language tags, most preferred first, separated by commas.
            #[arg(long, global = $global, value_name = "TAGS")]
            pub lang: Option<String>,

            /// Fail when a cached file does not match the lock file.
            #[arg(long, global = $global)]
            pub locked: bool,

            /// Lock file to read or write. Defaults to ontaria.lock.json with --locked.
            #[arg(long, global = $global, value_name = "FILE")]
            pub lock: Option<PathBuf>,

            /// Do not use the network. A missing cache entry is an error.
            #[arg(long, global = $global)]
            pub offline: bool,
        }

        impl From<$name> for LoadFlags {
            fn from(args: $name) -> Self {
                Self {
                    ontology: args.ontology,
                    imports: args.imports,
                    cache_dir: args.cache_dir,
                    catalog: args.catalog,
                    lang: args.lang,
                    locked: args.locked,
                    lock: args.lock,
                    offline: args.offline,
                }
            }
        }
    };
}

ontology_opts!(OntologyOpts, false);
ontology_opts!(OntologyOptsGlobal, true);

macro_rules! cache_opts {
    ($name:ident, $global:literal) => {
        #[derive(Args, Clone, Default)]
        #[command(next_help_heading = "Cache")]
        pub(crate) struct $name {
            /// Cache directory. Overrides ONTARIA_CACHE_DIR.
            #[arg(long, global = $global, value_name = "DIR")]
            pub cache_dir: Option<PathBuf>,

            /// Do not use the network. A missing cache entry is an error.
            #[arg(long, global = $global)]
            pub offline: bool,
        }

        impl From<$name> for LoadFlags {
            fn from(args: $name) -> Self {
                Self {
                    cache_dir: args.cache_dir,
                    offline: args.offline,
                    ..Self::default()
                }
            }
        }
    };
}

cache_opts!(CacheOpts, false);
cache_opts!(CacheOptsGlobal, true);

#[derive(Subcommand)]
pub(crate) enum Command {
    #[command(flatten)]
    Lookup(Lookup),
    #[command(flatten)]
    Check(Check),
    #[command(flatten)]
    Build(Build),
    #[command(flatten)]
    Store(Store),
    #[command(flatten)]
    Host(Host),
    #[command(flatten)]
    Story(Story),
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum Lookup {
    /// Show one class.
    GetClass {
        #[arg(value_name = "TERM")]
        term: String,
        #[arg(long)]
        inherited: bool,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Show one property.
    GetProperty {
        #[arg(value_name = "TERM")]
        term: String,
        #[arg(long)]
        usage: bool,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Show the superclasses of a class.
    GetParents {
        #[arg(value_name = "TERM")]
        term: String,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Show the subclasses of a class.
    GetChildren {
        #[arg(value_name = "TERM")]
        term: String,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Search classes and properties.
    Search {
        #[arg(value_name = "QUERY")]
        query: String,
        #[arg(long, value_enum, value_name = "KIND")]
        kind: Option<Kind>,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// List declared prefixes.
    ListNamespaces {
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// List owl:imports.
    ListImports {
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// List typed individuals.
    Resources {
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// List properties whose domain is a class.
    ListClassProperties {
        #[arg(value_name = "TERM")]
        term: String,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Look up several classes. A missing term is null.
    BatchGetClass {
        #[arg(value_name = "TERM", required = true)]
        terms: Vec<String>,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Look up several properties. A missing term is null.
    BatchGetProperty {
        #[arg(value_name = "TERM", required = true)]
        terms: Vec<String>,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Run several searches.
    BatchSearch {
        #[arg(value_name = "QUERY", required = true)]
        queries: Vec<String>,
        #[arg(long, value_enum, value_name = "KIND")]
        kind: Option<Kind>,
        #[command(flatten)]
        load: OntologyOpts,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum Kind {
    Class,
    Property,
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum Check {
    /// Validate instance data against the loaded ontology.
    Validate {
        #[arg(value_name = "DATA")]
        data: PathBuf,
        /// SHACL shapes file. Requires the `shacl` feature.
        #[arg(long, value_name = "SHAPES")]
        shacl: Option<PathBuf>,
        /// Print a Turtle ValidationReport instead of the JSON envelope.
        #[arg(long)]
        turtle: bool,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Lint an ontology.
    Lint {
        /// Exit 1 on warnings when the value is `warnings`.
        #[arg(long, value_name = "warnings")]
        deny: Option<String>,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Compare two ontology files.
    Diff {
        before: PathBuf,
        after: PathBuf,
        /// Exit 1 when the value is `breaking`.
        #[arg(long, value_name = "breaking")]
        fail_on: Option<String>,
    },
    /// Materialize RDFS entailments, or OWL 2 RL with --owl-rl.
    Reason {
        #[arg(long)]
        owl_rl: bool,
        #[command(flatten)]
        load: OntologyOpts,
    },
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum Build {
    /// Generate a context, bindings, schema, shapes, or docs.
    Gen {
        #[command(flatten)]
        load: OntologyOptsGlobal,
        #[command(subcommand)]
        target: GenTarget,
    },
    /// Check, apply, or reverse a YAML mapping.
    Map {
        #[command(subcommand)]
        command: MapCommand,
    },
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum Store {
    /// Download and cache an ontology. A local file path is cached too.
    Fetch {
        #[arg(value_name = "URL_OR_PATH")]
        url: String,
        /// Revalidate with ETag and Last-Modified when the cache already has the URL.
        #[arg(long)]
        refresh: bool,
        #[command(flatten)]
        cache: CacheOpts,
    },
    /// Cache maintenance.
    Cache {
        #[command(flatten)]
        cache: CacheOptsGlobal,
        #[command(subcommand)]
        command: CacheCommand,
    },
    /// List cached ontologies. `cache list` is the same command.
    #[command(hide = true)]
    ListCached {
        #[command(flatten)]
        cache: CacheOpts,
    },
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum Host {
    /// Read-only HTTP API.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
        #[command(flatten)]
        load: OntologyOpts,
    },
    /// Serve lookup, search, validate, and diff over MCP stdio.
    #[cfg(feature = "mcp")]
    Mcp {
        #[command(flatten)]
        load: OntologyOpts,
    },
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum Story {
    /// Validate or decode a Story Object Model 1.0 message.
    Som {
        #[command(flatten)]
        cache: CacheOptsGlobal,
        #[command(subcommand)]
        command: SomCommand,
    },
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum CacheCommand {
    List,
    /// Delete entries older than a duration such as 30d, 12h, or 15m.
    Prune {
        #[arg(long, value_name = "DURATION")]
        older_than: String,
    },
    Verify,
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum SomCommand {
    /// Validate a message with the upstream `som` crate.
    Validate {
        #[arg(value_name = "URL_OR_PATH")]
        message: PathBuf,
    },
    /// Validate a message, then decode it with the upstream `som` crate.
    Decode {
        #[arg(value_name = "URL_OR_PATH")]
        message: PathBuf,
    },
}

#[derive(Subcommand, Clone)]
#[command(rename_all = "kebab-case")]
pub(crate) enum MapCommand {
    /// Check a mapping against the loaded ontology.
    Validate {
        mapping: PathBuf,
        #[command(flatten)]
        load: OntologyOpts,
    },
    Apply {
        mapping: PathBuf,
        input: PathBuf,
    },
    Reverse {
        mapping: PathBuf,
        input: PathBuf,
    },
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum GenTarget {
    Context {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Rust {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Typescript {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Go {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Python {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    JsonSchema {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Shacl {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Pg {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    Docs {
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

/// Expand each argument. A pattern containing `*`, `?`, or `[` is walked and
/// its matching files are collected in glob order. A plain path passes through.
/// A pattern that matches nothing contributes no inputs.
pub(crate) fn expand_paths(patterns: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for pattern in patterns {
        if pattern.contains(['*', '?', '[']) {
            match glob::glob(pattern) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        if entry.is_file() {
                            out.push(entry.to_string_lossy().into_owned());
                        }
                    }
                }
                Err(_) => out.push(pattern.clone()),
            }
        } else {
            out.push(pattern.clone());
        }
    }
    out
}
