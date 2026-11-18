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

mod cache;
mod catalog;
mod error;
pub mod load;
mod manager;
mod model;
mod parser;
mod query;

pub use cache::{CacheManager, CacheMetadata, default_cache_dir};
pub use catalog::Catalog;
pub use error::{Error, Result};
pub use load::{LoadOptions, Loaded, existing_file, file_url, load_ontology};
pub use model::{
    Class, Ontology, Property, PropertyRef, PropertyType, Resource, ResourceValue, TermKind,
};
pub use parser::{RdfFormat, detect_format, parse_document, parse_turtle};
pub use query::{
    ClassDetails, ClassPropertiesResult, PropertyDetails, PropertyUsage, SearchResults,
};
