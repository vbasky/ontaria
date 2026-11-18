//! Parse an RDF Schema or OWL ontology and query it.
//!
//! Load a document once with [`Ontology::load`]. The value is immutable and
//! [`Send`] + [`Sync`], so it can be shared as `Arc<Ontology>`. Query methods
//! take `&self` and do no further I/O. This crate does not print.
//!
//! # Features
//!
//! * `fetch` — download ontologies with `ureq` (on by default).
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
//! The minimum supported Rust version is 1.88.

#![cfg_attr(docsrs, feature(doc_cfg))]

mod cache;
mod catalog;
mod codegen;
mod error;
mod extract;
mod load;
mod manager;
mod model;
mod parser;
mod query;
mod validate;
mod vocab;

pub use cache::{CacheManager, CacheMetadata, FetchLimits, VerifyReport, default_cache_dir};
pub use catalog::Catalog;
pub use codegen::{generate, jsonld_context, rust_source};
pub use error::{Error, Result, Warning, WarningCode};
pub use load::{LoadOptions, Loaded, existing_file, file_url, is_remote, load_ontology, parse_age};
pub use model::{
    Annotation, Class, ClassExpression, Concept, ConceptScheme, Fact, Individual, LangPref,
    LangString, Ontology, OntologyMetadata, Property, PropertyRef, PropertyType, Resource,
    ResourceValue, Statement, TermKind,
};
pub use parser::{RdfFormat, detect_format, parse_document, parse_turtle, parse_with_format};
pub use validate::{
    Severity, ValidationIssue, ValidationReport, validate_graph, validate_json, validate_rdf,
    validate_statements,
};

#[cfg(feature = "codegen")]
pub use ontaria_macros::terms;
pub use query::{
    ClassDetails, ClassPropertiesResult, PropertyDetails, PropertyUsage, SearchResults,
};
