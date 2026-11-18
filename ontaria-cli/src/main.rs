//! Ontaria command line. The library does the work. This crate only parses
//! arguments and prints the JSON envelope.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ontaria::{
    CacheManager, Class, LoadOptions, Ontology, Property, SearchResults, existing_file, file_url,
    load_ontology,
};

mod output;

use output::{print_error, print_success};

#[derive(Parser)]
#[command(
    name = "ontaria",
    version,
    about = "Look up classes and properties in an RDF Schema or OWL ontology"
)]
struct Cli {
    /// Ontology URL, or a path to a local file.
    /// When omitted, the only cached ontology is used.
    #[arg(short = 'o', long, global = true, value_name = "URL_OR_PATH")]
    ontology: Option<String>,

    /// Follow owl:imports while loading.
    #[arg(long, global = true)]
    imports: bool,

    /// Cache directory. Overrides ONTARIA_CACHE_DIR.
    #[arg(long, global = true, value_name = "DIR")]
    cache_dir: Option<PathBuf>,

    /// Offline catalog (TOML or OASIS XML) mapping import IRIs to local files.
    #[arg(long, global = true, value_name = "FILE")]
    catalog: Option<PathBuf>,

    /// Do not use the network. A missing cache entry is an error.
    #[arg(long, global = true)]
    offline: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Class,
    Property,
}

#[derive(Subcommand)]
#[command(rename_all = "kebab-case")]
enum Command {
    /// Download and cache an ontology. A local file path is cached too.
    Fetch {
        #[arg(value_name = "URL_OR_PATH")]
        url: String,
    },
    /// List cached ontologies.
    ListCached,
    GetClass {
        #[arg(value_name = "TERM")]
        term: String,
        #[arg(long)]
        inherited: bool,
    },
    GetProperty {
        #[arg(value_name = "TERM")]
        term: String,
        #[arg(long)]
        usage: bool,
    },
    GetParents {
        #[arg(value_name = "TERM")]
        term: String,
    },
    GetChildren {
        #[arg(value_name = "TERM")]
        term: String,
    },
    Search {
        #[arg(value_name = "QUERY")]
        query: String,
        #[arg(long, value_enum, value_name = "KIND")]
        kind: Option<Kind>,
    },
    ListNamespaces,
    ListImports,
    Resources,
    ListClassProperties {
        #[arg(value_name = "TERM")]
        term: String,
    },
    BatchGetClass {
        #[arg(value_name = "TERM", required = true)]
        terms: Vec<String>,
    },
    BatchGetProperty {
        #[arg(value_name = "TERM", required = true)]
        terms: Vec<String>,
    },
    BatchSearch {
        #[arg(value_name = "QUERY", required = true)]
        queries: Vec<String>,
        #[arg(long, value_enum, value_name = "KIND")]
        kind: Option<Kind>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if print_error(err.to_string()).is_err() {
                eprintln!("{err}");
            }
            ExitCode::from(1)
        }
    }
}

fn dispatch(cli: Cli) -> ontaria::Result<()> {
    let options = options_from(&cli);
    match cli.command {
        Command::Fetch { url } => fetch(&options, &url, cli.offline),
        Command::ListCached => list_cached(&options),
        Command::GetClass { term, inherited } => {
            get_class(&cli.ontology, &term, inherited, &options)
        }
        Command::GetProperty { term, usage } => get_property(&cli.ontology, &term, usage, &options),
        Command::GetParents { term } => parents(&cli.ontology, &term, &options),
        Command::GetChildren { term } => children(&cli.ontology, &term, &options),
        Command::Search { query, kind } => search(&cli.ontology, &query, kind, &options),
        Command::ListNamespaces => namespaces(&cli.ontology, &options),
        Command::ListImports => imports_of(&cli.ontology, &options),
        Command::Resources => resources(&cli.ontology, &options),
        Command::ListClassProperties { term } => class_properties(&cli.ontology, &term, &options),
        Command::BatchGetClass { terms } => batch_classes(&cli.ontology, &terms, &options),
        Command::BatchGetProperty { terms } => batch_properties(&cli.ontology, &terms, &options),
        Command::BatchSearch { queries, kind } => {
            batch_search(&cli.ontology, &queries, kind, &options)
        }
    }
}

fn options_from(cli: &Cli) -> LoadOptions {
    LoadOptions {
        follow_imports: cli.imports,
        cache_dir: cli.cache_dir.clone(),
        catalog_path: cli.catalog.clone(),
        allow_network: false,
    }
}

fn open_cache(options: &LoadOptions) -> ontaria::Result<CacheManager> {
    match &options.cache_dir {
        Some(dir) => CacheManager::open(dir),
        None => CacheManager::shared(),
    }
}

fn fetch(options: &LoadOptions, url: &str, offline: bool) -> ontaria::Result<()> {
    if offline {
        return Err(ontaria::Error::Message(
            "fetch needs the network; omit --offline".to_string(),
        ));
    }
    let cache = open_cache(options)?;
    let meta = if let Some(path) = existing_file(url) {
        let bytes = fs::read(&path).map_err(|source| ontaria::Error::Read {
            path: path.clone(),
            source,
        })?;
        cache.save(&file_url(&path), &bytes)?
    } else {
        cache.fetch(url)?
    };
    print_success(&meta, &[])
}

fn list_cached(options: &LoadOptions) -> ontaria::Result<()> {
    let cached = open_cache(options)?.list()?;
    print_success(&cached, &[])
}

fn get_class(
    spec: &Option<String>,
    term: &str,
    inherited: bool,
    options: &LoadOptions,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    if inherited {
        let details = ontology
            .get_class_with_inherited_properties(term)
            .ok_or_else(|| missing_err("class", term))?;
        return print_success(&details, &warnings);
    }
    let class = require_class(&ontology, term)?;
    print_success(class, &warnings)
}

fn get_property(
    spec: &Option<String>,
    term: &str,
    usage: bool,
    options: &LoadOptions,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    if usage {
        let usage = ontology
            .get_property_with_usage(term)
            .ok_or_else(|| missing_err("property", term))?;
        return print_success(&usage, &warnings);
    }
    let property = require_property(&ontology, term)?;
    print_success(property, &warnings)
}

fn parents(spec: &Option<String>, term: &str, options: &LoadOptions) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let parents = ontology
        .get_parents(term)
        .ok_or_else(|| missing_err("class", term))?;
    print_success(&parents, &warnings)
}

fn children(spec: &Option<String>, term: &str, options: &LoadOptions) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let children = ontology
        .get_children(term)
        .ok_or_else(|| missing_err("class", term))?;
    print_success(&children, &warnings)
}

fn search(
    spec: &Option<String>,
    query: &str,
    kind: Option<Kind>,
    options: &LoadOptions,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let results = filter_search(ontology.search(query), kind);
    print_success(&results, &warnings)
}

fn namespaces(spec: &Option<String>, options: &LoadOptions) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    print_success(&ontology.namespaces, &warnings)
}

fn imports_of(spec: &Option<String>, options: &LoadOptions) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    print_success(&ontology.imports, &warnings)
}

fn resources(spec: &Option<String>, options: &LoadOptions) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    print_success(&ontology.resources, &warnings)
}

fn class_properties(
    spec: &Option<String>,
    term: &str,
    options: &LoadOptions,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let result = ontology
        .get_class_properties(term)
        .ok_or_else(|| missing_err("class", term))?;
    print_success(&result, &warnings)
}

fn batch_classes(
    spec: &Option<String>,
    terms: &[String],
    options: &LoadOptions,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let mut map = serde_json::Map::new();
    for term in terms {
        let value = match ontology.get_class(term).cloned() {
            Some(class) => serde_json::to_value(class).unwrap_or(serde_json::Value::Null),
            None => serde_json::Value::Null,
        };
        map.insert(term.clone(), value);
    }
    print_success(&map, &warnings)
}

fn batch_properties(
    spec: &Option<String>,
    terms: &[String],
    options: &LoadOptions,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let mut map = serde_json::Map::new();
    for term in terms {
        let value = match ontology.get_property(term).cloned() {
            Some(property) => serde_json::to_value(property).unwrap_or(serde_json::Value::Null),
            None => serde_json::Value::Null,
        };
        map.insert(term.clone(), value);
    }
    print_success(&map, &warnings)
}

fn batch_search(
    spec: &Option<String>,
    queries: &[String],
    kind: Option<Kind>,
    options: &LoadOptions,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let mut map = serde_json::Map::new();
    for query in queries {
        let results = filter_search(ontology.search(query), kind);
        map.insert(
            query.clone(),
            serde_json::to_value(results).unwrap_or(serde_json::Value::Null),
        );
    }
    print_success(&map, &warnings)
}

fn ontology_for(
    spec: &Option<String>,
    options: &LoadOptions,
) -> ontaria::Result<(Ontology, Vec<String>)> {
    let loaded = load_ontology(spec.as_deref(), options)?;
    Ok((loaded.ontology, loaded.warnings))
}

fn filter_search(mut results: SearchResults, kind: Option<Kind>) -> SearchResults {
    match kind {
        Some(Kind::Class) => results.properties.clear(),
        Some(Kind::Property) => results.classes.clear(),
        None => {}
    }
    results
}

fn require_class<'a>(ontology: &'a Ontology, term: &str) -> ontaria::Result<&'a Class> {
    ontology
        .get_class(term)
        .ok_or_else(|| missing_err("class", term))
}

fn require_property<'a>(ontology: &'a Ontology, term: &str) -> ontaria::Result<&'a Property> {
    ontology
        .get_property(term)
        .ok_or_else(|| missing_err("property", term))
}

fn missing_err(kind: &str, name: &str) -> ontaria::Error {
    ontaria::Error::Message(format!("{kind} not found: {name}"))
}
