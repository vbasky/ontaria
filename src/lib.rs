//! Library for parsing an RDF Schema or OWL ontology in Turtle, RDF/XML, or
//! JSON-LD and answering class, property, and hierarchy questions about it.
//!
//! The command-line tool in `main.rs` is a thin wrapper over these types.
//! Cached ontology bytes live in `~/.ontaria/cache`.

mod cache;
mod error;
pub mod load;
mod manager;
mod model;
mod output;
mod parser;
mod query;

pub use cache::{CacheManager, CacheMetadata};
pub use error::{Error, Result};
pub use load::Loaded;
pub use manager::OntologyManager;
pub use model::{
    Class, Ontology, Property, PropertyRef, PropertyType, Resource, ResourceValue, TermKind,
};
pub use output::{print_error, print_success};
pub use parser::{RdfFormat, detect_format, parse_document, parse_turtle};
pub use query::{
    ClassDetails, ClassPropertiesResult, PropertyDetails, PropertyUsage, SearchResults,
};
