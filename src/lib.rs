//! Parse an RDF Schema or OWL ontology and query it.
//!
//! Load a document once with [`Ontology::load`]. The value is immutable and
//! [`Send`] + [`Sync`], so it can be shared as `Arc<Ontology>`. Query methods
//! take `&self` and do no further I/O. This crate does not print.
//!
//! # Features
//!
//! * `fetch` — download ontologies with `ureq` (on by default).
//! * `async-fetch` — the same download with `reqwest`. The caller supplies the runtime.
//! * `shacl` — SHACL validation through `shacl-rust`.
//! * `reasoning` — OWL 2 RL through `reasonable`.
//! * `codegen` — re-export the [`terms!`](crate::terms) proc macro.
//!
//! # Examples
//!
//! ```
//! let ttl = br#"
//!   @prefix ex: <http://example.org/> .
//!   ex:Person a <http://www.w3.org/2002/07/owl#Class> ;
//!     <http://www.w3.org/2000/01/rdf-schema#label> "Person" .
//! "#;
//! let ontology = ontaria::parse_document(ttl, "http://example.org/").unwrap();
//! assert_eq!(ontology.get_class("Person").unwrap().label, "Person");
//! ```
//!
//! The minimum supported Rust version is 1.88. See `STABILITY.md` for the
//! public-API policy.

#![cfg_attr(docsrs, feature(doc_cfg))]

mod cache;
mod catalog;
mod codegen;
mod diff;
mod ebucore;
mod error;
mod extract;
mod lint;
mod load;
mod lockfile;
mod manager;
mod mapping;
mod model;
mod parser;
mod query;
mod reason;
mod shacl;
mod validate;
mod vocab;

pub use cache::{CacheManager, CacheMetadata, FetchLimits, VerifyReport, default_cache_dir};
pub use catalog::Catalog;
pub use codegen::{
    generate, go_source, json_schema, jsonld_context, markdown_docs, property_graph, python_source,
    rust_source, shacl_shapes, typescript_source,
};
pub use diff::{ChangeKind, OntologyDiff, SemverBump, TermChange, diff};
pub use ebucore::is_ebucore_instance;
pub use error::{Error, Result, Warning, WarningCode};
pub use lint::{LintOptions, lint};
pub use load::{LoadOptions, Loaded, existing_file, file_url, is_remote, load_ontology, parse_age};
pub use lockfile::{LockEntry, LockFile};
pub use mapping::{FieldMapping, Mapping, apply, load_mapping, reverse, validate_mapping};
pub use model::{
    Annotation, Class, ClassExpression, Concept, ConceptScheme, Fact, Individual, LangPref,
    LangString, Ontology, OntologyMetadata, Property, PropertyRef, PropertyType, Resource,
    ResourceValue, Statement, TermKind,
};
pub use parser::{
    RdfFormat, detect_format, is_jsonld_document, parse_document, parse_turtle, parse_with_format,
};
pub use reason::{TypeFacts, apply_owl_rl, apply_rdfs, types_of};
pub use shacl::validate_shacl;
pub use validate::{
    Severity, ValidationIssue, ValidationReport, validate_graph, validate_json, validate_rdf,
    validate_statements,
};

#[cfg(feature = "codegen")]
pub use ontaria_macros::terms;
pub use query::{
    ClassDetails, ClassPropertiesResult, PropertyDetails, PropertyUsage, SearchResults,
};
