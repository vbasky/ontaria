//! Ontaria command line. The library does the work. This crate only parses
//! arguments and prints the JSON envelope.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use ontaria::{
    CacheManager, Class, FetchLimits, LangPref, LoadOptions, Ontology, Property, SearchResults,
    Warning, existing_file, file_url, is_remote, parse_age,
};

mod cli;
#[cfg(feature = "mcp")]
mod mcp;
mod output;
mod serve;
#[cfg(feature = "som")]
mod som_cmd;

#[cfg(feature = "som")]
use cli::Story;
use cli::{
    Build, CacheCommand, Check, Cli, GenTarget, Host, Kind, LoadFlags, Lookup, MapCommand, Store,
};
use output::{print_error, print_success};

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.command.is_none() && cli.paths.is_empty() && !LoadFlags::from(cli.load.clone()).any() {
        let _ = Cli::command().print_help();
        println!();
        return ExitCode::SUCCESS;
    }
    if let Some(path) = &cli.log_file
        && let Err(err) = output::set_log_file(path)
    {
        if print_error(err.to_string()).is_err() {
            eprintln!("{err}");
        }
        return ExitCode::from(1);
    }
    match dispatch(cli) {
        Ok(code) => code,
        Err(err) => {
            if print_error(err.to_string()).is_err() {
                eprintln!("{err}");
            }
            ExitCode::from(1)
        }
    }
}

fn dispatch(cli: Cli) -> ontaria::Result<ExitCode> {
    let root = LoadFlags::from(cli.load);
    let paths = cli.paths;
    match cli.command {
        None => return describe_paths(&paths, &root),
        Some(cli::Command::Lookup(command)) => lookup(&paths, &root, command)?,
        Some(cli::Command::Check(command)) => return check(&paths, &root, command),
        Some(cli::Command::Build(command)) => return build(&paths, &root, command),
        Some(cli::Command::Store(command)) => return store(&paths, &root, command),
        Some(cli::Command::Host(command)) => return host(&paths, &root, command),
        #[cfg(feature = "som")]
        Some(cli::Command::Story(Story::Som { cache, command })) => {
            let flags = cache_invocation("som", &paths, &root, cache.into())?;
            return som_cmd::run(command, flags.offline, flags.cache_dir.as_deref());
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn lookup(paths: &[String], root: &LoadFlags, command: Lookup) -> ontaria::Result<()> {
    match command {
        Lookup::GetClass {
            term,
            inherited,
            load,
        } => {
            let (ontology, options) = ontology_invocation("get-class", paths, root, load.into())?;
            get_class(&ontology, &term, inherited, &options)
        }
        Lookup::GetProperty { term, usage, load } => {
            let (ontology, options) =
                ontology_invocation("get-property", paths, root, load.into())?;
            get_property(&ontology, &term, usage, &options)
        }
        Lookup::GetParents { term, load } => {
            let (ontology, options) = ontology_invocation("get-parents", paths, root, load.into())?;
            parents(&ontology, &term, &options)
        }
        Lookup::GetChildren { term, load } => {
            let (ontology, options) =
                ontology_invocation("get-children", paths, root, load.into())?;
            children(&ontology, &term, &options)
        }
        Lookup::Search { query, kind, load } => {
            let (ontology, options) = ontology_invocation("search", paths, root, load.into())?;
            search(&ontology, &query, kind, &options)
        }
        Lookup::ListNamespaces { load } => {
            let (ontology, options) =
                ontology_invocation("list-namespaces", paths, root, load.into())?;
            namespaces(&ontology, &options)
        }
        Lookup::ListImports { load } => {
            let (ontology, options) =
                ontology_invocation("list-imports", paths, root, load.into())?;
            imports_of(&ontology, &options)
        }
        Lookup::Resources { load } => {
            let (ontology, options) = ontology_invocation("resources", paths, root, load.into())?;
            resources(&ontology, &options)
        }
        Lookup::ListClassProperties { term, load } => {
            let (ontology, options) =
                ontology_invocation("list-class-properties", paths, root, load.into())?;
            class_properties(&ontology, &term, &options)
        }
        Lookup::BatchGetClass { terms, load } => {
            let (ontology, options) =
                ontology_invocation("batch-get-class", paths, root, load.into())?;
            batch_classes(&ontology, &terms, &options)
        }
        Lookup::BatchGetProperty { terms, load } => {
            let (ontology, options) =
                ontology_invocation("batch-get-property", paths, root, load.into())?;
            batch_properties(&ontology, &terms, &options)
        }
        Lookup::BatchSearch {
            queries,
            kind,
            load,
        } => {
            let (ontology, options) =
                ontology_invocation("batch-search", paths, root, load.into())?;
            batch_search(&ontology, &queries, kind, &options)
        }
    }
}

fn check(paths: &[String], root: &LoadFlags, command: Check) -> ontaria::Result<ExitCode> {
    match command {
        Check::Validate {
            data,
            shacl,
            turtle,
            load,
        } => {
            let (ontology, options) = ontology_invocation("validate", paths, root, load.into())?;
            validate(&ontology, &options, &data, shacl.as_deref(), turtle)
        }
        Check::Lint { deny, load } => {
            let (ontology, options) = ontology_invocation("lint", paths, root, load.into())?;
            lint_file(&ontology, &options, deny.as_deref())
        }
        Check::Diff {
            before,
            after,
            fail_on,
        } => {
            reject_ontology("diff", paths, root)?;
            diff_files(&before, &after, fail_on.as_deref())
        }
        Check::Reason { owl_rl, load } => {
            let (ontology, options) = ontology_invocation("reason", paths, root, load.into())?;
            reason(&ontology, &options, owl_rl)
        }
    }
}

fn build(paths: &[String], root: &LoadFlags, command: Build) -> ontaria::Result<ExitCode> {
    match command {
        Build::Gen { load, target } => {
            let (ontology, options) = ontology_invocation("gen", paths, root, load.into())?;
            generate(&ontology, &options, target)?;
            Ok(ExitCode::SUCCESS)
        }
        Build::Map { command } => map_dispatch(paths, root, command),
    }
}

fn store(paths: &[String], root: &LoadFlags, command: Store) -> ontaria::Result<ExitCode> {
    match command {
        Store::Fetch {
            url,
            refresh,
            cache,
        } => {
            let flags = cache_invocation("fetch", paths, root, cache.into())?;
            fetch(&options_from(&flags), &url, flags.offline, refresh)?;
            Ok(ExitCode::SUCCESS)
        }
        Store::Cache { cache, command } => {
            let flags = cache_invocation("cache", paths, root, cache.into())?;
            cache_command(&options_from(&flags), command)
        }
        Store::ListCached { cache } => {
            let flags = cache_invocation("list-cached", paths, root, cache.into())?;
            list_cached(&options_from(&flags))?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn host(paths: &[String], root: &LoadFlags, command: Host) -> ontaria::Result<ExitCode> {
    match command {
        Host::Serve { bind, load } => {
            let (ontology, options) = ontology_invocation("serve", paths, root, load.into())?;
            let (loaded, _) = ontology_for(&ontology, &options)?;
            serve::serve(loaded, &bind)
        }
        #[cfg(feature = "mcp")]
        Host::Mcp { load } => {
            let (ontology, options) = ontology_invocation("mcp", paths, root, load.into())?;
            let (loaded, _) = ontology_for(&ontology, &options)?;
            mcp::serve(loaded)
        }
    }
}

fn map_dispatch(
    paths: &[String],
    root: &LoadFlags,
    command: MapCommand,
) -> ontaria::Result<ExitCode> {
    match command {
        MapCommand::Validate { mapping, load } => {
            let (ontology, options) =
                ontology_invocation("map validate", paths, root, load.into())?;
            map_validate(&ontology, &options, &mapping)
        }
        MapCommand::Apply { mapping, input } => {
            reject_ontology("map apply", paths, root)?;
            let mapping = read_mapping(&mapping)?;
            let value = read_json(&input)?;
            print_success(ontaria::apply(&mapping, &value)?, &[])?;
            Ok(ExitCode::SUCCESS)
        }
        MapCommand::Reverse { mapping, input } => {
            reject_ontology("map reverse", paths, root)?;
            let mapping = read_mapping(&mapping)?;
            let value = read_json(&input)?;
            print_success(ontaria::reverse(&mapping, &value)?, &[])?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn ontology_invocation(
    command: &str,
    patterns: &[String],
    root: &LoadFlags,
    child: LoadFlags,
) -> ontaria::Result<(Option<String>, LoadOptions)> {
    let flags = merge_flags(root, &child)?;
    let paths = expanded_paths(patterns)?;
    let spec = match (paths.len(), flags.ontology.clone()) {
        (0, spec) => spec,
        (1, None) => paths.into_iter().next(),
        (1, Some(spec)) if same_input(&paths[0], &spec) => Some(spec),
        (_, Some(_)) => {
            return Err(ontaria::Error::message(
                "PATH and --ontology name different files",
            ));
        }
        _ => {
            return Err(ontaria::Error::message(format!(
                "{command} takes one ontology PATH"
            )));
        }
    };
    Ok((spec, options_from(&flags)))
}

fn cache_invocation(
    command: &str,
    patterns: &[String],
    root: &LoadFlags,
    child: LoadFlags,
) -> ontaria::Result<LoadFlags> {
    if !patterns.is_empty() || root.ontology_only() || child.ontology_only() {
        return Err(ontaria::Error::message(format!(
            "{command} does not take ontology options"
        )));
    }
    merge_flags(root, &child)
}

fn reject_ontology(command: &str, patterns: &[String], root: &LoadFlags) -> ontaria::Result<()> {
    if patterns.is_empty() && !root.any() {
        return Ok(());
    }
    Err(ontaria::Error::message(format!(
        "{command} does not take ontology options"
    )))
}

fn merge_flags(root: &LoadFlags, child: &LoadFlags) -> ontaria::Result<LoadFlags> {
    Ok(LoadFlags {
        ontology: merge_value(root.ontology.clone(), child.ontology.clone(), "--ontology")?,
        imports: root.imports || child.imports,
        cache_dir: merge_value(
            root.cache_dir.clone(),
            child.cache_dir.clone(),
            "--cache-dir",
        )?,
        catalog: merge_value(root.catalog.clone(), child.catalog.clone(), "--catalog")?,
        lang: merge_value(root.lang.clone(), child.lang.clone(), "--lang")?,
        locked: root.locked || child.locked,
        lock: merge_value(root.lock.clone(), child.lock.clone(), "--lock")?,
        offline: root.offline || child.offline,
    })
}

fn merge_value<T: Clone + PartialEq>(
    left: Option<T>,
    right: Option<T>,
    flag: &str,
) -> ontaria::Result<Option<T>> {
    match (left, right) {
        (Some(left), Some(right)) if left != right => Err(ontaria::Error::message(format!(
            "{flag} is set twice with different values"
        ))),
        (Some(left), _) => Ok(Some(left)),
        (None, right) => Ok(right),
    }
}

fn expanded_paths(patterns: &[String]) -> ontaria::Result<Vec<String>> {
    let paths = cli::expand_paths(patterns);
    if !patterns.is_empty() && paths.is_empty() {
        return Err(ontaria::Error::message(format!(
            "no files matched: {}",
            patterns.join(", ")
        )));
    }
    Ok(paths)
}

fn same_input(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn describe_paths(patterns: &[String], flags: &LoadFlags) -> ontaria::Result<ExitCode> {
    let mut inputs = expanded_paths(patterns)?;
    if let Some(spec) = &flags.ontology
        && !inputs.iter().any(|path| same_input(path, spec))
    {
        inputs.insert(0, spec.clone());
    }
    if inputs.is_empty() {
        return Err(ontaria::Error::message(format!(
            "no files matched: {}",
            patterns.join(", ")
        )));
    }
    let options = options_from(flags);
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    for path in inputs {
        let ontology = Ontology::load(&path, &options)?;
        warnings.extend(ontology.warnings.clone());
        rows.push(serde_json::json!({
            "path": path,
            "format": ontology.format,
            "classes": ontology.classes.len(),
            "properties": ontology.properties.len(),
            "namespaces": ontology.namespaces.len(),
        }));
    }
    let data = if rows.len() == 1 {
        rows.pop().unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Array(rows)
    };
    print_success(&data, &warnings)?;
    Ok(ExitCode::SUCCESS)
}

fn options_from(flags: &LoadFlags) -> LoadOptions {
    let tags = flags
        .lang
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string);
    LoadOptions {
        follow_imports: flags.imports,
        cache_dir: flags.cache_dir.clone(),
        catalog_path: flags.catalog.clone(),
        lang: if flags.lang.is_some() {
            LangPref::new(tags)
        } else {
            LangPref::default()
        },
        locked: flags.locked,
        lock_path: flags.lock.clone(),
        allow_network: false,
        ..LoadOptions::default()
    }
}

fn open_cache(options: &LoadOptions) -> ontaria::Result<CacheManager> {
    match &options.cache_dir {
        Some(dir) => CacheManager::open(dir),
        None => CacheManager::shared(),
    }
}

fn fetch(options: &LoadOptions, url: &str, offline: bool, refresh: bool) -> ontaria::Result<()> {
    let cache = open_cache(options)?;
    let meta = if let Some(path) = existing_file(url) {
        let bytes = fs::read(&path).map_err(|source| ontaria::Error::Read {
            path: path.clone(),
            source,
        })?;
        cache.save(&file_url(&path), &bytes)?
    } else if is_remote(url) {
        if offline {
            return Err(ontaria::Error::message(
                "fetch needs the network; omit --offline",
            ));
        }
        cache.fetch_limited(url, &FetchLimits::default(), refresh)?
    } else {
        return Err(ontaria::Error::message(format!(
            "ontology not cached: {url} (run 'fetch' first)"
        )));
    };
    print_success(&meta, &[] as &[Warning])
}

fn cache_command(options: &LoadOptions, command: CacheCommand) -> ontaria::Result<ExitCode> {
    let cache = open_cache(options)?;
    match command {
        CacheCommand::List => print_success(cache.list()?, &[])?,
        CacheCommand::Prune { older_than } => {
            let removed = cache.prune(parse_age(&older_than)?)?;
            print_success(&removed, &[])?;
        }
        CacheCommand::Verify => print_success(cache.verify()?, &[])?,
    }
    Ok(ExitCode::SUCCESS)
}

fn list_cached(options: &LoadOptions) -> ontaria::Result<()> {
    let cached = open_cache(options)?.list()?;
    print_success(&cached, &[] as &[Warning])
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
) -> ontaria::Result<(Ontology, Vec<Warning>)> {
    let spec = match spec.clone().filter(|value| !value.is_empty()) {
        Some(spec) => spec,
        None => {
            let cache = open_cache(options)?;
            match cache.list()?.as_slice() {
                [] => {
                    return Err(ontaria::Error::message(
                        "no cached ontologies found, use --ontology or fetch one first",
                    ));
                }
                [only] => {
                    let mut ontology = Ontology::load(&only.url, options)?;
                    ontology.warnings.insert(
                        0,
                        Warning::new(
                            ontaria::WarningCode::Other,
                            format!("Using cached ontology: {}", only.url),
                        ),
                    );
                    let warnings = ontology.warnings.clone();
                    return Ok((ontology, warnings));
                }
                _ => {
                    return Err(ontaria::Error::message(
                        "multiple ontologies cached, specify --ontology",
                    ));
                }
            }
        }
    };
    let ontology = Ontology::load(&spec, options)?;
    let warnings = ontology.warnings.clone();
    Ok((ontology, warnings))
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

fn validate(
    spec: &Option<String>,
    options: &LoadOptions,
    data: &Path,
    shapes: Option<&Path>,
    turtle: bool,
) -> ontaria::Result<ExitCode> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let bytes = fs::read(data).map_err(|source| ontaria::Error::Read {
        path: data.to_path_buf(),
        source,
    })?;
    let mut report = if plain_json(&bytes) {
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|err| ontaria::Error::message(format!("invalid JSON: {err}")))?;
        ontaria::validate_json(&ontology, &value)
    } else {
        ontaria::validate_rdf(&ontology, &bytes, &data.display().to_string())?
    };
    if let Some(shapes) = shapes {
        let shape_bytes = fs::read(shapes).map_err(|source| ontaria::Error::Read {
            path: shapes.to_path_buf(),
            source,
        })?;
        let shape_ontology = ontaria::parse_document(&shape_bytes, &shapes.display().to_string())?;
        let data_ontology = if plain_json(&bytes) {
            return Err(ontaria::Error::message(
                "SHACL validation needs RDF instance data, not JSON",
            ));
        } else {
            ontaria::parse_document(&bytes, &data.display().to_string())?
        };
        let shacl = ontaria::validate_shacl(&shape_ontology, &data_ontology)?;
        report.issues.extend(shacl.issues);
    }
    let failed = report.has_errors();
    if turtle {
        print_success(report.to_turtle(), &warnings)?;
    } else {
        print_success(&report, &warnings)?;
    }
    Ok(if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn generate(
    spec: &Option<String>,
    options: &LoadOptions,
    target: GenTarget,
) -> ontaria::Result<()> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    match target {
        GenTarget::Context { out } => emit(
            ontaria::jsonld_context(&ontology),
            out.as_deref(),
            &warnings,
        ),
        GenTarget::Rust { out } => {
            emit_text(&ontaria::rust_source(&ontology), out.as_deref(), &warnings)
        }
        GenTarget::Typescript { out } => emit_text(
            &ontaria::typescript_source(&ontology),
            out.as_deref(),
            &warnings,
        ),
        GenTarget::Go { out } => {
            emit_text(&ontaria::go_source(&ontology), out.as_deref(), &warnings)
        }
        GenTarget::Python { out } => emit_text(
            &ontaria::python_source(&ontology),
            out.as_deref(),
            &warnings,
        ),
        GenTarget::JsonSchema { out } => {
            emit(ontaria::json_schema(&ontology), out.as_deref(), &warnings)
        }
        GenTarget::Shacl { out } => {
            emit_text(&ontaria::shacl_shapes(&ontology), out.as_deref(), &warnings)
        }
        GenTarget::Pg { out } => emit_text(
            &ontaria::property_graph(&ontology),
            out.as_deref(),
            &warnings,
        ),
        GenTarget::Docs { out } => emit_text(
            &ontaria::markdown_docs(&ontology),
            out.as_deref(),
            &warnings,
        ),
    }
}

fn emit(
    value: impl serde::Serialize,
    out: Option<&Path>,
    warnings: &[Warning],
) -> ontaria::Result<()> {
    if let Some(path) = out {
        let text = serde_json::to_string_pretty(&value)
            .map_err(|err| ontaria::Error::message(format!("failed to encode JSON: {err}")))?;
        fs::write(path, text).map_err(|source| ontaria::Error::Write {
            path: path.to_path_buf(),
            source,
        })?;
        print_success(path.display().to_string(), warnings)
    } else {
        print_success(&value, warnings)
    }
}

fn emit_text(text: &str, out: Option<&Path>, warnings: &[Warning]) -> ontaria::Result<()> {
    if let Some(path) = out {
        fs::write(path, text).map_err(|source| ontaria::Error::Write {
            path: path.to_path_buf(),
            source,
        })?;
        print_success(path.display().to_string(), warnings)
    } else {
        print_success(text, warnings)
    }
}

fn diff_files(before: &Path, after: &Path, fail_on: Option<&str>) -> ontaria::Result<ExitCode> {
    let left = read_ontology(before)?;
    let right = read_ontology(after)?;
    let report = ontaria::diff(&left, &right);
    let fail = fail_on == Some("breaking") && report.has_breaking();
    print_success(&report, &[])?;
    Ok(if fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn lint_file(
    spec: &Option<String>,
    options: &LoadOptions,
    deny: Option<&str>,
) -> ontaria::Result<ExitCode> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let deny_warnings = deny == Some("warnings");
    let report = ontaria::lint(&ontology, &ontaria::LintOptions { deny_warnings });
    let fail = report.has_errors() || (deny_warnings && !report.issues.is_empty());
    print_success(&report, &warnings)?;
    Ok(if fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn reason(spec: &Option<String>, options: &LoadOptions, owl_rl: bool) -> ontaria::Result<ExitCode> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let entailed = if owl_rl {
        ontaria::apply_owl_rl(&ontology)?
    } else {
        ontaria::apply_rdfs(&ontology)
    };
    let inferred = entailed
        .statements
        .iter()
        .filter(|statement| statement.inferred)
        .cloned()
        .collect::<Vec<_>>();
    print_success(&inferred, &warnings)?;
    Ok(ExitCode::SUCCESS)
}

fn map_validate(
    spec: &Option<String>,
    options: &LoadOptions,
    mapping: &Path,
) -> ontaria::Result<ExitCode> {
    let (ontology, warnings) = ontology_for(spec, options)?;
    let mapping = read_mapping(mapping)?;
    let report = ontaria::validate_mapping(&ontology, &mapping);
    let fail = report.has_errors();
    print_success(&report, &warnings)?;
    Ok(if fail {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn read_mapping(path: &Path) -> ontaria::Result<ontaria::Mapping> {
    let text = fs::read_to_string(path).map_err(|source| ontaria::Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    ontaria::load_mapping(&text)
}

fn read_json(path: &Path) -> ontaria::Result<serde_json::Value> {
    let bytes = fs::read(path).map_err(|source| ontaria::Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|err| ontaria::Error::message(format!("invalid JSON: {err}")))
}

fn read_ontology(path: &Path) -> ontaria::Result<Ontology> {
    let bytes = fs::read(path).map_err(|source| ontaria::Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    ontaria::parse_document(&bytes, &path.display().to_string())
}

fn looks_like_json(bytes: &[u8]) -> bool {
    matches!(
        bytes.iter().find(|byte| !byte.is_ascii_whitespace()),
        Some(b'{' | b'[')
    )
}

/// JSON that is not JSON-LD and not an EBUCore or MediaInfo instance.
/// Those two are parsed to triples so `@context` and `#value` are kept.
fn plain_json(bytes: &[u8]) -> bool {
    looks_like_json(bytes)
        && !ontaria::is_jsonld_document(bytes)
        && !ontaria::is_ebucore_instance(bytes)
}

fn missing_err(kind: &str, name: &str) -> ontaria::Error {
    ontaria::Error::NotFound {
        kind: kind.to_string(),
        term: name.to_string(),
    }
}
