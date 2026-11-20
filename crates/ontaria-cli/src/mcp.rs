//! MCP server. Tools are lookup, search, hierarchy, validate, and diff.
//!
//! Logging goes to stderr. stdout is the MCP transport.

use std::sync::Arc;

use ontaria::Ontology;
use rmcp::{ServiceExt, handler::server::wrapper::Parameters, tool, tool_router, transport::stdio};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone)]
struct Server {
    ontology: Arc<Ontology>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TermArgs {
    /// IRI, prefix:Name, local name, or label.
    term: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchArgs {
    query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ValidateArgs {
    /// JSON instance document.
    json: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DiffArgs {
    before: String,
    after: String,
}

#[tool_router(server_handler)]
impl Server {
    #[tool(description = "Look up one class or property by IRI, prefix, local name, or label")]
    async fn lookup(&self, Parameters(args): Parameters<TermArgs>) -> String {
        if let Some(class) = self.ontology.get_class(&args.term) {
            return serde_json::to_string(class).unwrap_or_else(|err| err.to_string());
        }
        if let Some(property) = self.ontology.get_property(&args.term) {
            return serde_json::to_string(property).unwrap_or_else(|err| err.to_string());
        }
        format!("term not found: {}", args.term)
    }

    #[tool(description = "Search classes, properties, concepts, and individuals")]
    async fn search(&self, Parameters(args): Parameters<SearchArgs>) -> String {
        serde_json::to_string(&self.ontology.search(&args.query))
            .unwrap_or_else(|err| err.to_string())
    }

    #[tool(description = "List parent and child classes")]
    async fn hierarchy(&self, Parameters(args): Parameters<TermArgs>) -> String {
        let parents = self.ontology.get_parents(&args.term);
        let children = self.ontology.get_children(&args.term);
        serde_json::to_string(&json!({"parents": parents, "children": children}))
            .unwrap_or_else(|err| err.to_string())
    }

    #[tool(
        description = "Validate a JSON or JSON-LD instance against the loaded ontology. JSON-LD is expanded first."
    )]
    async fn validate(&self, Parameters(args): Parameters<ValidateArgs>) -> String {
        let bytes = args.json.as_bytes();
        let report = if ontaria::is_jsonld_document(bytes) || ontaria::is_ebucore_instance(bytes) {
            match ontaria::validate_rdf(&self.ontology, bytes, "http://ontaria.local/instance") {
                Ok(report) => report,
                Err(err) => return err.to_string(),
            }
        } else {
            let value = match serde_json::from_str::<serde_json::Value>(&args.json) {
                Ok(value) => value,
                Err(err) => return format!("invalid JSON: {err}"),
            };
            ontaria::validate_json(&self.ontology, &value)
        };
        serde_json::to_string(&report).unwrap_or_else(|err| err.to_string())
    }

    #[tool(description = "Diff two ontology documents passed as Turtle text")]
    async fn diff(&self, Parameters(args): Parameters<DiffArgs>) -> String {
        let before =
            match ontaria::parse_document(args.before.as_bytes(), "http://ontaria.local/before") {
                Ok(ontology) => ontology,
                Err(err) => return err.to_string(),
            };
        let after =
            match ontaria::parse_document(args.after.as_bytes(), "http://ontaria.local/after") {
                Ok(ontology) => ontology,
                Err(err) => return err.to_string(),
            };
        let _ = &self.ontology;
        serde_json::to_string(&ontaria::diff(&before, &after)).unwrap_or_else(|err| err.to_string())
    }
}

pub fn serve(ontology: Ontology) -> ontaria::Result<std::process::ExitCode> {
    let server = Server {
        ontology: Arc::new(ontology),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| ontaria::Error::message(format!("failed to start tokio: {err}")))?;
    runtime.block_on(async move {
        eprintln!("ontaria mcp server on stdio");
        let service = server
            .serve(stdio())
            .await
            .map_err(|err| ontaria::Error::message(format!("mcp serve failed: {err}")))?;
        service
            .waiting()
            .await
            .map_err(|err| ontaria::Error::message(format!("mcp server stopped: {err}")))?;
        Ok(std::process::ExitCode::SUCCESS)
    })
}
