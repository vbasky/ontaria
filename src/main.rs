use std::fs;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ontaria::{
    CacheManager, Class, Ontology, OntologyManager, Property, SearchResults, load::existing_file,
    load::file_url, load::load_ontology, print_error, print_success,
};

#[derive(Parser)]
#[command(
    name = "ontaria",
    version,
    about = "Look up classes and properties in an RDF Schema or OWL ontology"
)]
struct Cli {
    /// Ontology URL, or a path to a local Turtle file.
    /// When omitted, the only cached ontology is used.
    #[arg(short, long, global = true, value_name = "URL_OR_PATH")]
    ontology: Option<String>,

    /// Follow owl:imports when a command looks up a term or searches.
    #[arg(long, global = true)]
    imports: bool,

    #[command(subcommand)]
    command: Command,
}

/// Which side of a search to keep. Omit it to return both.
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
        /// Ontology URL, or a path to a local Turtle file.
        #[arg(value_name = "URL_OR_PATH")]
        url: String,
    },
    /// List cached ontologies.
    ListCached,
    /// Get one class.
    GetClass {
        /// IRI, `prefix:Local`, local name, or label.
        #[arg(value_name = "TERM")]
        term: String,
        /// Include properties inherited from parent classes.
        #[arg(long)]
        inherited: bool,
    },
    /// Get one property.
    GetProperty {
        /// IRI, `prefix:Local`, local name, or label.
        #[arg(value_name = "TERM")]
        term: String,
        /// Include the classes in the domain and range.
        #[arg(long)]
        usage: bool,
    },
    /// Get the parent classes of a class.
    GetParents {
        /// IRI, `prefix:Local`, local name, or label.
        #[arg(value_name = "TERM")]
        term: String,
    },
    /// Get the child classes of a class.
    GetChildren {
        /// IRI, `prefix:Local`, local name, or label.
        #[arg(value_name = "TERM")]
        term: String,
    },
    /// Search classes and properties.
    Search {
        /// Text matched against labels, comments, descriptions, and IRIs.
        #[arg(value_name = "QUERY")]
        query: String,
        /// Return only classes or only properties.
        #[arg(long, value_enum, value_name = "KIND")]
        kind: Option<Kind>,
    },
    /// List namespace prefixes.
    ListNamespaces,
    /// List owl:imports declarations.
    ListImports,
    /// List the typed things in the file, such as a news article.
    Resources,
    /// List properties of a class, properties that reference it, and inherited properties.
    ListClassProperties {
        /// IRI, `prefix:Local`, local name, or label.
        #[arg(value_name = "TERM")]
        term: String,
    },
    /// Get several classes. The result is a map keyed by each term.
    BatchGetClass {
        /// One or more terms. Each may be an IRI, `prefix:Local`, local name, or label.
        #[arg(value_name = "TERM", required = true)]
        terms: Vec<String>,
    },
    /// Get several properties. The result is a map keyed by each term.
    BatchGetProperty {
        /// One or more terms. Each may be an IRI, `prefix:Local`, local name, or label.
        #[arg(value_name = "TERM", required = true)]
        terms: Vec<String>,
    },
    /// Search for several queries. The result is a map keyed by each query.
    BatchSearch {
        /// One or more queries, matched against labels, comments, descriptions, and IRIs.
        #[arg(value_name = "QUERY", required = true)]
        queries: Vec<String>,
        /// Return only classes or only properties.
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
    let ontology = &cli.ontology;
    let imports = cli.imports;
    match cli.command {
        Command::Fetch { url } => fetch(&url),
        Command::ListCached => list_cached(),
        Command::GetClass { term, inherited } => get_class(ontology, &term, inherited, imports),
        Command::GetProperty { term, usage } => get_property(ontology, &term, usage, imports),
        Command::GetParents { term } => parents(ontology, &term, imports),
        Command::GetChildren { term } => children(ontology, &term, imports),
        Command::Search { query, kind } => search(ontology, &query, kind, imports),
        Command::ListNamespaces => namespaces(ontology, imports),
        Command::ListImports => imports_of(ontology, imports),
        Command::Resources => resources(ontology, imports),
        Command::ListClassProperties { term } => class_properties(ontology, &term, imports),
        Command::BatchGetClass { terms } => batch_classes(ontology, &terms, imports),
        Command::BatchGetProperty { terms } => batch_properties(ontology, &terms, imports),
        Command::BatchSearch { queries, kind } => batch_search(ontology, &queries, kind, imports),
    }
}

fn fetch(url: &str) -> ontaria::Result<()> {
    let cache = CacheManager::shared()?;
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

fn list_cached() -> ontaria::Result<()> {
    let cached = CacheManager::shared()?.list()?;
    print_success(&cached, &[])
}

fn get_class(
    spec: &Option<String>,
    term: &str,
    inherited: bool,
    imports: bool,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
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
    imports: bool,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
    if usage {
        let usage = ontology
            .get_property_with_usage(term)
            .ok_or_else(|| missing_err("property", term))?;
        return print_success(&usage, &warnings);
    }
    let property = require_property(&ontology, term)?;
    print_success(property, &warnings)
}

fn parents(spec: &Option<String>, term: &str, imports: bool) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
    let parents = ontology
        .get_parents(term)
        .ok_or_else(|| missing_err("class", term))?;
    print_success(&parents, &warnings)
}

fn children(spec: &Option<String>, term: &str, imports: bool) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
    let children = ontology
        .get_children(term)
        .ok_or_else(|| missing_err("class", term))?;
    print_success(&children, &warnings)
}

fn search(
    spec: &Option<String>,
    query: &str,
    kind: Option<Kind>,
    imports: bool,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
    let results = filter_search(ontology.search(query), kind);
    print_success(&results, &warnings)
}

fn namespaces(spec: &Option<String>, imports: bool) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
    print_success(&ontology.namespaces, &warnings)
}

fn imports_of(spec: &Option<String>, imports: bool) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
    print_success(&ontology.imports, &warnings)
}

fn resources(spec: &Option<String>, imports: bool) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
    print_success(&ontology.resources, &warnings)
}

fn class_properties(spec: &Option<String>, term: &str, imports: bool) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
    let result = ontology
        .get_class_properties(term)
        .ok_or_else(|| missing_err("class", term))?;
    print_success(&result, &warnings)
}

fn batch_classes(spec: &Option<String>, terms: &[String], imports: bool) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
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

fn batch_properties(spec: &Option<String>, terms: &[String], imports: bool) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
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
    imports: bool,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, imports)?;
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

fn ontology_for(spec: &Option<String>, imports: bool) -> ontaria::Result<(Ontology, Vec<String>)> {
    let loaded = load_ontology(spec.as_deref())?;
    if !imports {
        return Ok((loaded.ontology, loaded.warnings));
    }
    let mut manager = OntologyManager::new(loaded.ontology, loaded.cache);
    let ontology = manager.merged();
    Ok((
        ontology,
        join_warnings(&loaded.warnings, manager.warnings()),
    ))
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

fn join_warnings(first: &[String], second: &[String]) -> Vec<String> {
    let mut warnings = first.to_vec();
    warnings.extend(second.iter().cloned());
    warnings
}
