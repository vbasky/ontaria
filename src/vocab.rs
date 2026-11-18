//! IRIs the parser, validator, and linter all share.

pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub const OWL: &str = "http://www.w3.org/2002/07/owl#";
pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
pub const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
pub const SHACL: &str = "http://www.w3.org/ns/shacl#";
pub const DCTERMS: &str = "http://purl.org/dc/terms/";
pub const DC: &str = "http://purl.org/dc/elements/1.1/";
pub const PROV: &str = "http://www.w3.org/ns/prov#";

pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
#[allow(dead_code)]
pub const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

pub const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
pub const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
pub const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
pub const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
pub const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
pub const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
pub const RDFS_SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
pub const RDFS_SEE_ALSO: &str = "http://www.w3.org/2000/01/rdf-schema#seeAlso";
pub const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
#[allow(dead_code)]
pub const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";

pub const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
pub const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
pub const OWL_OBJECT: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
pub const OWL_DATATYPE: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
pub const OWL_ANNOTATION: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
pub const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
pub const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
pub const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
pub const OWL_ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
pub const OWL_ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
pub const OWL_SOME: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
pub const OWL_ALL: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
pub const OWL_HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
pub const OWL_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
pub const OWL_MIN: &str = "http://www.w3.org/2002/07/owl#minCardinality";
pub const OWL_MAX: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
pub const OWL_QUAL_CARD: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
pub const OWL_MIN_QUAL: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
pub const OWL_MAX_QUAL: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
pub const OWL_DEPRECATED: &str = "http://www.w3.org/2002/07/owl#deprecated";
pub const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
pub const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
pub const OWL_DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
pub const OWL_UNION: &str = "http://www.w3.org/2002/07/owl#unionOf";
pub const OWL_INTERSECTION: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
pub const OWL_COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#complementOf";
pub const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
pub const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
pub const OWL_INVERSE: &str = "http://www.w3.org/2002/07/owl#inverseOf";
pub const OWL_FUNCTIONAL: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
pub const OWL_INVERSE_FUNCTIONAL: &str = "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
pub const OWL_TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
pub const OWL_SYMMETRIC: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
pub const OWL_VERSION_IRI: &str = "http://www.w3.org/2002/07/owl#versionIRI";
pub const OWL_VERSION_INFO: &str = "http://www.w3.org/2002/07/owl#versionInfo";
pub const OWL_PRIOR_VERSION: &str = "http://www.w3.org/2002/07/owl#priorVersion";

pub const DCTERMS_DESCRIPTION: &str = "http://purl.org/dc/terms/description";
pub const DCTERMS_CREATED: &str = "http://purl.org/dc/terms/created";
pub const DCTERMS_MODIFIED: &str = "http://purl.org/dc/terms/modified";
pub const DCTERMS_LICENSE: &str = "http://purl.org/dc/terms/license";
pub const DCTERMS_RIGHTS: &str = "http://purl.org/dc/terms/rights";

pub const SKOS_CONCEPT: &str = "http://www.w3.org/2004/02/skos/core#Concept";
pub const SKOS_SCHEME: &str = "http://www.w3.org/2004/02/skos/core#ConceptScheme";
pub const SKOS_PREF: &str = "http://www.w3.org/2004/02/skos/core#prefLabel";
pub const SKOS_ALT: &str = "http://www.w3.org/2004/02/skos/core#altLabel";
pub const SKOS_HIDDEN: &str = "http://www.w3.org/2004/02/skos/core#hiddenLabel";
pub const SKOS_BROADER: &str = "http://www.w3.org/2004/02/skos/core#broader";
pub const SKOS_NARROWER: &str = "http://www.w3.org/2004/02/skos/core#narrower";
pub const SKOS_RELATED: &str = "http://www.w3.org/2004/02/skos/core#related";
pub const SKOS_EXACT: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
pub const SKOS_CLOSE: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
pub const SKOS_IN_SCHEME: &str = "http://www.w3.org/2004/02/skos/core#inScheme";
pub const SKOS_TOP: &str = "http://www.w3.org/2004/02/skos/core#hasTopConcept";

pub const SCHEMA_DOMAIN: &[&str] = &[
    "https://schema.org/domainIncludes",
    "http://schema.org/domainIncludes",
];
pub const SCHEMA_RANGE: &[&str] = &[
    "https://schema.org/rangeIncludes",
    "http://schema.org/rangeIncludes",
];

pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// Vocabulary namespaces that are not "unknown" when they appear as a type or predicate.
pub fn is_builtin_namespace(iri: &str) -> bool {
    iri.starts_with(RDF)
        || iri.starts_with(RDFS)
        || iri.starts_with(OWL)
        || iri.starts_with(XSD)
        || iri.starts_with(SKOS)
        || iri.starts_with(SHACL)
        || iri.starts_with(DCTERMS)
        || iri.starts_with(DC)
        || iri.starts_with(PROV)
}

pub fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}
