//! Index triples into the ontology model.

use std::collections::HashMap;

use oxrdf::{BlankNode, Literal, NamedNode, NamedOrBlankNode, Term, Triple};

use crate::error::{Warning, WarningCode};
use crate::model::{
    Annotation, Class, ClassExpression, Concept, ConceptScheme, Individual, LangString, Ontology,
    Property, PropertyRef, PropertyType, Resource, ResourceValue, Statement, TermKind, push_unique,
};
use crate::vocab::{
    self, DCTERMS_CREATED, DCTERMS_DESCRIPTION, DCTERMS_LICENSE, DCTERMS_MODIFIED, DCTERMS_RIGHTS,
    OWL_ALL, OWL_ANNOTATION, OWL_CARDINALITY, OWL_CLASS, OWL_COMPLEMENT, OWL_DATATYPE,
    OWL_DEPRECATED, OWL_DISJOINT, OWL_EQUIVALENT_CLASS, OWL_EQUIVALENT_PROPERTY, OWL_FUNCTIONAL,
    OWL_HAS_VALUE, OWL_IMPORTS, OWL_INTERSECTION, OWL_INVERSE, OWL_INVERSE_FUNCTIONAL, OWL_MAX,
    OWL_MAX_QUAL, OWL_MIN, OWL_MIN_QUAL, OWL_NAMED_INDIVIDUAL, OWL_OBJECT, OWL_ON_CLASS,
    OWL_ON_DATA_RANGE, OWL_ON_PROPERTY, OWL_ONE_OF, OWL_ONTOLOGY, OWL_PRIOR_VERSION, OWL_QUAL_CARD,
    OWL_RESTRICTION, OWL_SOME, OWL_SYMMETRIC, OWL_TRANSITIVE, OWL_UNION, OWL_VERSION_INFO,
    OWL_VERSION_IRI, RDF_FIRST, RDF_NIL, RDF_PROPERTY, RDF_REST, RDF_TYPE, RDFS_CLASS,
    RDFS_COMMENT, RDFS_DOMAIN, RDFS_IS_DEFINED_BY, RDFS_LABEL, RDFS_RANGE, RDFS_SEE_ALSO,
    RDFS_SUBCLASS, RDFS_SUBPROPERTY, SCHEMA_DOMAIN, SCHEMA_RANGE, SKOS_ALT, SKOS_BROADER,
    SKOS_CLOSE, SKOS_CONCEPT, SKOS_EXACT, SKOS_HIDDEN, SKOS_IN_SCHEME, SKOS_NARROWER, SKOS_PREF,
    SKOS_RELATED, SKOS_SCHEME, SKOS_TOP, XSD_STRING,
};

pub(crate) struct InputTriple {
    pub triple: Triple,
    pub graph: Option<String>,
}

pub(crate) fn build_ontology(
    base_uri: &str,
    format: &str,
    triples: Vec<InputTriple>,
    prefixes: Vec<(String, String)>,
) -> Ontology {
    let mut ontology = Ontology::new(base_uri);
    ontology.format = format.to_string();
    for (prefix, iri) in &prefixes {
        ontology.namespaces.insert(prefix.clone(), iri.clone());
        ontology
            .declared_prefixes
            .insert(prefix.clone(), iri.clone());
    }
    let mut graphs = Vec::new();
    let mut plain = Vec::with_capacity(triples.len());
    for item in &triples {
        if let Some(graph) = &item.graph {
            push_unique(&mut graphs, graph.clone());
        }
        plain.push(item.triple.clone());
        ontology
            .statements
            .push(statement_from(&item.triple, item.graph.as_deref()));
    }
    if graphs.len() > 1 {
        ontology.warnings.push(Warning::new(
            WarningCode::NamedGraphsMerged,
            format!(
                "merged {} named graphs into one ontology: {}",
                graphs.len(),
                graphs.join(", ")
            ),
        ));
    }
    let index = Index::build(plain);
    extract_metadata(&index, &mut ontology);
    extract_classes(&index, &mut ontology);
    extract_properties(&index, &mut ontology);
    link_hierarchy(&index, &mut ontology);
    link_properties(&index, &mut ontology);
    extract_restrictions(&index, &mut ontology);
    extract_imports(&index, &mut ontology);
    extract_individuals(&index, &mut ontology);
    extract_skos(&index, &mut ontology);
    extract_resources(&index, &mut ontology);
    ontology.apply_language(&crate::model::LangPref::default());
    ontology
}

fn statement_from(triple: &Triple, graph: Option<&str>) -> Statement {
    let (object, kind, datatype, language) = term_parts(&triple.object);
    Statement {
        subject: node_id(&triple.subject),
        predicate: triple.predicate.as_str().to_string(),
        object,
        kind,
        datatype,
        language,
        graph: graph.map(str::to_string),
        inferred: false,
    }
}

struct Index {
    by_subject: HashMap<NamedOrBlankNode, Vec<Triple>>,
}

impl Index {
    fn build(triples: Vec<Triple>) -> Self {
        let mut by_subject: HashMap<NamedOrBlankNode, Vec<Triple>> = HashMap::new();
        for triple in triples {
            by_subject
                .entry(triple.subject.clone())
                .or_default()
                .push(triple);
        }
        Self { by_subject }
    }

    fn objects<'a>(&'a self, subject: &NamedOrBlankNode, predicate: &str) -> Vec<&'a Term> {
        self.by_subject
            .get(subject)
            .map(|triples| {
                triples
                    .iter()
                    .filter(|triple| triple.predicate.as_str() == predicate)
                    .map(|triple| &triple.object)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn iris(&self, subject: &NamedOrBlankNode, predicate: &str) -> Vec<String> {
        self.objects(subject, predicate)
            .into_iter()
            .filter_map(term_iri)
            .collect()
    }

    fn lang_strings(&self, subject: &NamedOrBlankNode, predicate: &str) -> Vec<LangString> {
        self.objects(subject, predicate)
            .into_iter()
            .filter_map(|term| match term {
                Term::Literal(literal) => Some(lang_string(literal)),
                _ => None,
            })
            .collect()
    }

    fn literal(&self, subject: &NamedOrBlankNode, predicate: &str) -> String {
        preferred_text(&self.lang_strings(subject, predicate))
    }
}

fn extract_metadata(index: &Index, ontology: &mut Ontology) {
    for (subject, triples) in &index.by_subject {
        if !typed_as(triples, OWL_ONTOLOGY) {
            continue;
        }
        let meta = &mut ontology.metadata;
        if let Some(iri) = index.iris(subject, OWL_VERSION_IRI).into_iter().next() {
            meta.version_iri = Some(iri);
        }
        let version = index.literal(subject, OWL_VERSION_INFO);
        if !version.is_empty() {
            meta.version_info = Some(version);
        }
        if let Some(iri) = index.iris(subject, OWL_PRIOR_VERSION).into_iter().next() {
            meta.prior_version = Some(iri);
        }
        let created = index.literal(subject, DCTERMS_CREATED);
        if !created.is_empty() {
            meta.created = Some(created);
        }
        let modified = index.literal(subject, DCTERMS_MODIFIED);
        if !modified.is_empty() {
            meta.modified = Some(modified);
        }
        if let Some(license) = first_non_empty([
            index.iris(subject, DCTERMS_LICENSE).into_iter().next(),
            index.iris(subject, DCTERMS_RIGHTS).into_iter().next(),
            nonempty(index.literal(subject, DCTERMS_LICENSE)),
            nonempty(index.literal(subject, DCTERMS_RIGHTS)),
        ]) {
            meta.license = Some(license);
        }
        meta.labels = index.lang_strings(subject, RDFS_LABEL);
        meta.comments = index.lang_strings(subject, RDFS_COMMENT);
        meta.see_also = index.iris(subject, RDFS_SEE_ALSO);
        meta.annotations = annotations(index, subject, &annotation_properties(index));
    }
}

fn extract_classes(index: &Index, ontology: &mut Ontology) {
    let annotation_iris = annotation_properties(index);
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(node) = subject else {
            continue;
        };
        if !typed_as(triples, OWL_CLASS) && !typed_as(triples, RDFS_CLASS) {
            continue;
        }
        let uri = node.as_str().to_string();
        let mut class = Class::named(uri.clone(), ontology.uri.clone());
        class.labels = index.lang_strings(subject, RDFS_LABEL);
        class.comments = index.lang_strings(subject, RDFS_COMMENT);
        class.descriptions = index.lang_strings(subject, DCTERMS_DESCRIPTION);
        class.label = preferred_text(&class.labels);
        class.comment = preferred_text(&class.comments);
        class.description = preferred_text(&class.descriptions);
        class.deprecated = is_deprecated(index, subject);
        class.see_also = index.iris(subject, RDFS_SEE_ALSO);
        class.is_defined_by = index.iris(subject, RDFS_IS_DEFINED_BY);
        class.disjoint_with = index.iris(subject, OWL_DISJOINT);
        class.one_of = individuals_of(index, subject, OWL_ONE_OF);
        class.union_of = expressions_of(index, subject, OWL_UNION);
        class.intersection_of = expressions_of(index, subject, OWL_INTERSECTION);
        class.complement_of = index
            .objects(subject, OWL_COMPLEMENT)
            .into_iter()
            .next()
            .map(|term| expression(index, term));
        for term in index.objects(subject, OWL_EQUIVALENT_CLASS) {
            match term {
                Term::NamedNode(node) => {
                    push_unique(&mut class.equivalent_classes, node.as_str().to_string());
                }
                other => class.equivalent_expressions.push(expression(index, other)),
            }
        }
        class.annotations = annotations(index, subject, &annotation_iris);
        ontology.classes.insert(uri, class);
    }
}

fn extract_properties(index: &Index, ontology: &mut Ontology) {
    let annotation_iris = annotation_properties(index);
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(node) = subject else {
            continue;
        };
        let Some(kind) = property_type(triples) else {
            continue;
        };
        let uri = node.as_str().to_string();
        let mut property = Property::named(uri.clone(), ontology.uri.clone(), kind);
        property.labels = index.lang_strings(subject, RDFS_LABEL);
        property.comments = index.lang_strings(subject, RDFS_COMMENT);
        property.descriptions = index.lang_strings(subject, DCTERMS_DESCRIPTION);
        property.label = preferred_text(&property.labels);
        property.comment = preferred_text(&property.comments);
        property.description = preferred_text(&property.descriptions);
        property.deprecated = is_deprecated(index, subject);
        property.see_also = index.iris(subject, RDFS_SEE_ALSO);
        property.is_defined_by = index.iris(subject, RDFS_IS_DEFINED_BY);
        property.functional = typed_as(triples, OWL_FUNCTIONAL);
        property.inverse_functional = typed_as(triples, OWL_INVERSE_FUNCTIONAL);
        property.transitive = typed_as(triples, OWL_TRANSITIVE);
        property.symmetric = typed_as(triples, OWL_SYMMETRIC);
        property.equivalent_properties = index.iris(subject, OWL_EQUIVALENT_PROPERTY);
        property.inverse_of = index.iris(subject, OWL_INVERSE);
        collect_class_targets(
            index,
            subject,
            RDFS_DOMAIN,
            &mut property.domain,
            &mut property.domain_expressions,
        );
        collect_class_targets(
            index,
            subject,
            RDFS_RANGE,
            &mut property.range,
            &mut property.range_expressions,
        );
        for predicate in SCHEMA_DOMAIN {
            for iri in index.iris(subject, predicate) {
                push_unique(&mut property.domain, iri);
            }
        }
        for predicate in SCHEMA_RANGE {
            for iri in index.iris(subject, predicate) {
                push_unique(&mut property.range, iri);
            }
        }
        property.annotations = annotations(index, subject, &annotation_iris);
        ontology.properties.insert(uri, property);
    }
}

fn collect_class_targets(
    index: &Index,
    subject: &NamedOrBlankNode,
    predicate: &str,
    iris: &mut Vec<String>,
    expressions: &mut Vec<ClassExpression>,
) {
    for term in index.objects(subject, predicate) {
        match term {
            Term::NamedNode(node) => push_unique(iris, node.as_str().to_string()),
            other => expressions.push(expression(index, other)),
        }
    }
}

fn link_hierarchy(index: &Index, ontology: &mut Ontology) {
    let mut edges: Vec<(String, String)> = Vec::new();
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(child) = subject else {
            continue;
        };
        for triple in triples {
            if triple.predicate.as_str() != RDFS_SUBCLASS {
                continue;
            }
            let Term::NamedNode(parent) = &triple.object else {
                continue;
            };
            edges.push((child.as_str().to_string(), parent.as_str().to_string()));
        }
    }
    for (child_uri, parent_uri) in edges {
        if let Some(child) = ontology.classes.get_mut(&child_uri) {
            push_unique(&mut child.parents, parent_uri.clone());
        }
        if let Some(parent) = ontology.classes.get_mut(&parent_uri) {
            push_unique(&mut parent.children, child_uri);
        }
    }
}

fn link_properties(index: &Index, ontology: &mut Ontology) {
    let mut edges: Vec<(String, String)> = Vec::new();
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(child) = subject else {
            continue;
        };
        for triple in triples {
            if triple.predicate.as_str() != RDFS_SUBPROPERTY {
                continue;
            }
            let Term::NamedNode(parent) = &triple.object else {
                continue;
            };
            edges.push((child.as_str().to_string(), parent.as_str().to_string()));
        }
    }
    for (child_uri, parent_uri) in edges {
        if let Some(child) = ontology.properties.get_mut(&child_uri) {
            push_unique(&mut child.super_properties, parent_uri.clone());
        }
        if let Some(parent) = ontology.properties.get_mut(&parent_uri) {
            push_unique(&mut parent.sub_properties, child_uri);
        }
    }
}

fn extract_restrictions(index: &Index, ontology: &mut Ontology) {
    let mut on_property: HashMap<BlankNode, PropertyRef> = HashMap::new();
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::BlankNode(blank) = subject else {
            continue;
        };
        if !typed_as(triples, OWL_RESTRICTION) {
            continue;
        }
        let Some(property) = index.iris(subject, OWL_ON_PROPERTY).into_iter().next() else {
            continue;
        };
        on_property.insert(blank.clone(), restriction_ref(index, subject, property));
    }

    let mut links: Vec<(String, PropertyRef)> = Vec::new();
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(class_node) = subject else {
            continue;
        };
        for triple in triples {
            if triple.predicate.as_str() != RDFS_SUBCLASS {
                continue;
            }
            let Term::BlankNode(blank) = &triple.object else {
                continue;
            };
            if let Some(restriction) = on_property.get(blank) {
                let mut restriction = restriction.clone();
                restriction.source_class = class_node.as_str().to_string();
                links.push((class_node.as_str().to_string(), restriction));
            }
        }
    }
    for (class_uri, restriction) in links {
        let Some(class) = ontology.classes.get_mut(&class_uri) else {
            continue;
        };
        if let Some(existing) = class
            .properties
            .iter_mut()
            .find(|item| item.uri == restriction.uri)
        {
            merge_ref(existing, &restriction);
        } else {
            class.properties.push(restriction);
        }
    }
}

fn restriction_ref(index: &Index, subject: &NamedOrBlankNode, property: String) -> PropertyRef {
    let exact = first_number(index, subject, &[OWL_CARDINALITY, OWL_QUAL_CARD]);
    let min = first_number(index, subject, &[OWL_MIN, OWL_MIN_QUAL]);
    let max = first_number(index, subject, &[OWL_MAX, OWL_MAX_QUAL]);
    let some_values_from = index.iris(subject, OWL_SOME).into_iter().next();
    let all_values_from = index.iris(subject, OWL_ALL).into_iter().next();
    let has_value = index
        .objects(subject, OWL_HAS_VALUE)
        .into_iter()
        .next()
        .map(term_text);
    let on_class = index
        .iris(subject, OWL_ON_CLASS)
        .into_iter()
        .next()
        .or_else(|| index.iris(subject, OWL_ON_DATA_RANGE).into_iter().next());
    let required = exact.is_some_and(|value| value >= 1)
        || min.is_some_and(|value| value >= 1)
        || some_values_from.is_some()
        || has_value.is_some();
    let text = match (exact, min, max) {
        (Some(value), _, _) => value.to_string(),
        (None, Some(min), Some(max)) => format!("{min}..{max}"),
        (None, Some(min), None) => format!("{min}..*"),
        (None, None, Some(max)) => format!("0..{max}"),
        (None, None, None) if some_values_from.is_some() || has_value.is_some() => {
            "1..*".to_string()
        }
        (None, None, None) => String::new(),
    };
    PropertyRef {
        uri: property,
        required,
        cardinality: text,
        source_class: String::new(),
        on_class,
        some_values_from,
        all_values_from,
        has_value,
        min,
        max,
        exact,
    }
}

fn merge_ref(slot: &mut PropertyRef, extra: &PropertyRef) {
    slot.required |= extra.required;
    if slot.cardinality.is_empty() {
        slot.cardinality.clone_from(&extra.cardinality);
    }
    if slot.on_class.is_none() {
        slot.on_class.clone_from(&extra.on_class);
    }
    if slot.some_values_from.is_none() {
        slot.some_values_from.clone_from(&extra.some_values_from);
    }
    if slot.all_values_from.is_none() {
        slot.all_values_from.clone_from(&extra.all_values_from);
    }
    if slot.has_value.is_none() {
        slot.has_value.clone_from(&extra.has_value);
    }
    if slot.min.is_none() {
        slot.min = extra.min;
    }
    if slot.max.is_none() {
        slot.max = extra.max;
    }
    if slot.exact.is_none() {
        slot.exact = extra.exact;
    }
}

fn extract_individuals(index: &Index, ontology: &mut Ontology) {
    let classes: Vec<String> = ontology.classes.keys().cloned().collect();
    for subject in index.by_subject.keys() {
        let NamedOrBlankNode::NamedNode(node) = subject else {
            continue;
        };
        let uri = node.as_str().to_string();
        if ontology.classes.contains_key(&uri) || ontology.properties.contains_key(&uri) {
            continue;
        }
        let types = index.iris(subject, RDF_TYPE);
        let named = types.iter().any(|iri| iri == OWL_NAMED_INDIVIDUAL);
        let typed_as_class = types
            .iter()
            .any(|iri| classes.iter().any(|class| class == iri));
        if !named && !typed_as_class {
            continue;
        }
        let labels = index.lang_strings(subject, RDFS_LABEL);
        ontology.individuals.insert(
            uri.clone(),
            Individual {
                uri,
                types,
                label: preferred_text(&labels),
                labels,
                annotations: annotations(index, subject, &annotation_properties(index)),
            },
        );
    }
}

fn extract_skos(index: &Index, ontology: &mut Ontology) {
    for (subject, triples) in &index.by_subject {
        let NamedOrBlankNode::NamedNode(node) = subject else {
            continue;
        };
        let uri = node.as_str().to_string();
        if typed_as(triples, SKOS_SCHEME) {
            let labels = index.lang_strings(subject, RDFS_LABEL);
            let mut pref = index.lang_strings(subject, SKOS_PREF);
            if pref.is_empty() {
                pref.clone_from(&labels);
            }
            ontology.schemes.insert(
                uri.clone(),
                ConceptScheme {
                    uri: uri.clone(),
                    label: preferred_text(&pref),
                    labels: pref,
                    top_concepts: index.iris(subject, SKOS_TOP),
                },
            );
        }
        if typed_as(triples, SKOS_CONCEPT) {
            let pref_labels = index.lang_strings(subject, SKOS_PREF);
            ontology.concepts.insert(
                uri.clone(),
                Concept {
                    uri,
                    schemes: index.iris(subject, SKOS_IN_SCHEME),
                    pref_label: preferred_text(&pref_labels),
                    pref_labels,
                    alt_labels: index.lang_strings(subject, SKOS_ALT),
                    hidden_labels: index.lang_strings(subject, SKOS_HIDDEN),
                    broader: index.iris(subject, SKOS_BROADER),
                    narrower: index.iris(subject, SKOS_NARROWER),
                    related: index.iris(subject, SKOS_RELATED),
                    exact_match: index.iris(subject, SKOS_EXACT),
                    close_match: index.iris(subject, SKOS_CLOSE),
                    annotations: annotations(index, subject, &annotation_properties(index)),
                },
            );
        }
    }
}

fn extract_resources(index: &Index, ontology: &mut Ontology) {
    let mut resources = Vec::new();
    for (subject, triples) in &index.by_subject {
        let id = node_id(subject);
        let mut types = Vec::new();
        let mut values = Vec::new();
        for triple in triples {
            let predicate = triple.predicate.as_str();
            if predicate == RDF_TYPE {
                if let Some(iri) = term_iri(&triple.object) {
                    types.push(iri);
                }
                continue;
            }
            values.push(resource_value(predicate, &triple.object));
        }
        if types.is_empty() && values.is_empty() {
            continue;
        }
        types.sort();
        values.sort_by(|left, right| {
            left.predicate
                .cmp(&right.predicate)
                .then(left.value.cmp(&right.value))
        });
        resources.push(Resource { id, types, values });
    }
    resources.sort_by(|left, right| left.id.cmp(&right.id));
    ontology.resources = resources;
}

fn extract_imports(index: &Index, ontology: &mut Ontology) {
    for triples in index.by_subject.values() {
        for triple in triples {
            if triple.predicate.as_str() != OWL_IMPORTS {
                continue;
            }
            if let Some(iri) = term_iri(&triple.object) {
                push_unique(&mut ontology.imports, iri);
            }
        }
    }
}

fn resource_value(predicate: &str, object: &Term) -> ResourceValue {
    let (value, kind, datatype, language) = term_parts(object);
    ResourceValue {
        predicate: predicate.to_string(),
        value,
        kind,
        datatype,
        language,
    }
}

fn term_parts(object: &Term) -> (String, TermKind, Option<String>, Option<String>) {
    match object {
        Term::NamedNode(node) => (node.as_str().to_string(), TermKind::Iri, None, None),
        Term::BlankNode(node) => (format!("_:{}", node.as_str()), TermKind::Blank, None, None),
        Term::Literal(literal) => {
            let datatype = literal.datatype();
            let datatype = (datatype.as_str() != XSD_STRING).then(|| datatype.as_str().to_string());
            (
                literal.value().to_string(),
                TermKind::Literal,
                datatype,
                literal.language().map(str::to_string),
            )
        }
        #[allow(unreachable_patterns)]
        _ => (object.to_string(), TermKind::Literal, None, None),
    }
}

fn expression(index: &Index, term: &Term) -> ClassExpression {
    match term {
        Term::NamedNode(node) => ClassExpression::Class {
            iri: node.as_str().to_string(),
        },
        Term::BlankNode(node) => {
            let subject = NamedOrBlankNode::BlankNode(node.clone());
            if let Some(items) = list_expressions(index, &subject, OWL_UNION) {
                return ClassExpression::Union { operands: items };
            }
            if let Some(items) = list_expressions(index, &subject, OWL_INTERSECTION) {
                return ClassExpression::Intersection { operands: items };
            }
            if let Some(operand) = index.objects(&subject, OWL_COMPLEMENT).into_iter().next() {
                return ClassExpression::Complement {
                    operand: Box::new(expression(index, operand)),
                };
            }
            let individuals = individuals_of(index, &subject, OWL_ONE_OF);
            if !individuals.is_empty() {
                return ClassExpression::OneOf { individuals };
            }
            ClassExpression::Class {
                iri: format!("_:{}", node.as_str()),
            }
        }
        Term::Literal(literal) => ClassExpression::Class {
            iri: literal.value().to_string(),
        },
        #[allow(unreachable_patterns)]
        _ => ClassExpression::Class {
            iri: term.to_string(),
        },
    }
}

fn expressions_of(
    index: &Index,
    subject: &NamedOrBlankNode,
    predicate: &str,
) -> Vec<ClassExpression> {
    index
        .objects(subject, predicate)
        .into_iter()
        .flat_map(|term| match term {
            Term::BlankNode(_) | Term::NamedNode(_) => rdf_list(index, term)
                .into_iter()
                .map(|item| expression(index, &item))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

fn list_expressions(
    index: &Index,
    subject: &NamedOrBlankNode,
    predicate: &str,
) -> Option<Vec<ClassExpression>> {
    let head = index.objects(subject, predicate).into_iter().next()?;
    Some(
        rdf_list(index, head)
            .iter()
            .map(|term| expression(index, term))
            .collect(),
    )
}

fn individuals_of(index: &Index, subject: &NamedOrBlankNode, predicate: &str) -> Vec<String> {
    index
        .objects(subject, predicate)
        .into_iter()
        .flat_map(|term| rdf_list(index, term))
        .filter_map(|term| term_iri(&term))
        .collect()
}

fn rdf_list(index: &Index, head: &Term) -> Vec<Term> {
    let mut items = Vec::new();
    let mut node = match head {
        Term::NamedNode(node) if node.as_str() == RDF_NIL => return items,
        Term::NamedNode(node) => NamedOrBlankNode::NamedNode(node.clone()),
        Term::BlankNode(node) => NamedOrBlankNode::BlankNode(node.clone()),
        _ => return items,
    };
    for _ in 0..10_000 {
        if let NamedOrBlankNode::NamedNode(node) = &node
            && node.as_str() == RDF_NIL
        {
            break;
        }
        let Some(first) = index.objects(&node, RDF_FIRST).into_iter().next() else {
            break;
        };
        items.push((*first).clone());
        let Some(rest) = index.objects(&node, RDF_REST).into_iter().next() else {
            break;
        };
        node = match rest {
            Term::NamedNode(node) if node.as_str() == RDF_NIL => break,
            Term::NamedNode(node) => NamedOrBlankNode::NamedNode(node.clone()),
            Term::BlankNode(node) => NamedOrBlankNode::BlankNode(node.clone()),
            _ => break,
        };
    }
    items
}

fn annotations(
    index: &Index,
    subject: &NamedOrBlankNode,
    annotation_iris: &[String],
) -> Vec<Annotation> {
    let Some(triples) = index.by_subject.get(subject) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for triple in triples {
        let predicate = triple.predicate.as_str();
        if structural_predicate(predicate) || !annotation_iris.iter().any(|iri| iri == predicate) {
            continue;
        }
        let (value, kind, datatype, language) = term_parts(&triple.object);
        found.push(Annotation {
            property: predicate.to_string(),
            value,
            kind,
            datatype,
            language,
        });
    }
    found.sort_by(|left, right| {
        left.property
            .cmp(&right.property)
            .then(left.value.cmp(&right.value))
    });
    found
}

fn annotation_properties(index: &Index) -> Vec<String> {
    let mut iris = vec![
        RDFS_LABEL.to_string(),
        RDFS_COMMENT.to_string(),
        RDFS_SEE_ALSO.to_string(),
        RDFS_IS_DEFINED_BY.to_string(),
        OWL_DEPRECATED.to_string(),
        OWL_VERSION_INFO.to_string(),
        DCTERMS_DESCRIPTION.to_string(),
        DCTERMS_CREATED.to_string(),
        DCTERMS_MODIFIED.to_string(),
        DCTERMS_LICENSE.to_string(),
        DCTERMS_RIGHTS.to_string(),
        SKOS_PREF.to_string(),
        SKOS_ALT.to_string(),
        SKOS_HIDDEN.to_string(),
    ];
    for (subject, triples) in &index.by_subject {
        if typed_as(triples, OWL_ANNOTATION)
            && let NamedOrBlankNode::NamedNode(node) = subject
        {
            push_unique(&mut iris, node.as_str().to_string());
        }
    }
    iris
}

fn structural_predicate(predicate: &str) -> bool {
    matches!(
        predicate,
        RDF_TYPE
            | RDFS_LABEL
            | RDFS_COMMENT
            | RDFS_SEE_ALSO
            | RDFS_IS_DEFINED_BY
            | RDFS_DOMAIN
            | RDFS_RANGE
            | RDFS_SUBCLASS
            | RDFS_SUBPROPERTY
            | DCTERMS_DESCRIPTION
            | OWL_DEPRECATED
            | OWL_IMPORTS
            | OWL_EQUIVALENT_CLASS
            | OWL_DISJOINT
            | OWL_UNION
            | OWL_INTERSECTION
            | OWL_COMPLEMENT
            | OWL_ONE_OF
            | OWL_EQUIVALENT_PROPERTY
            | OWL_INVERSE
            | OWL_ON_PROPERTY
            | OWL_SOME
            | OWL_ALL
            | OWL_HAS_VALUE
            | OWL_CARDINALITY
            | OWL_MIN
            | OWL_MAX
            | OWL_QUAL_CARD
            | OWL_MIN_QUAL
            | OWL_MAX_QUAL
            | OWL_ON_CLASS
            | OWL_ON_DATA_RANGE
            | OWL_VERSION_IRI
            | OWL_VERSION_INFO
            | OWL_PRIOR_VERSION
            | SKOS_PREF
            | SKOS_ALT
            | SKOS_HIDDEN
            | SKOS_BROADER
            | SKOS_NARROWER
            | SKOS_RELATED
            | SKOS_EXACT
            | SKOS_CLOSE
            | SKOS_IN_SCHEME
            | SKOS_TOP
    ) || SCHEMA_DOMAIN.contains(&predicate)
        || SCHEMA_RANGE.contains(&predicate)
}

fn first_number(index: &Index, subject: &NamedOrBlankNode, predicates: &[&str]) -> Option<u64> {
    for predicate in predicates {
        for term in index.objects(subject, predicate) {
            let Term::Literal(literal) = term else {
                continue;
            };
            if let Ok(value) = literal.value().parse::<u64>() {
                return Some(value);
            }
        }
    }
    None
}

fn is_deprecated(index: &Index, subject: &NamedOrBlankNode) -> bool {
    index
        .objects(subject, OWL_DEPRECATED)
        .iter()
        .any(|term| match term {
            Term::Literal(literal) => literal.value().eq_ignore_ascii_case("true"),
            _ => false,
        })
}

/// More specific OWL property types beat a bare `rdf:Property`.
fn property_type(triples: &[Triple]) -> Option<PropertyType> {
    let mut best: Option<(u8, PropertyType)> = None;
    for triple in triples {
        if triple.predicate.as_str() != RDF_TYPE {
            continue;
        }
        let Some(iri) = term_iri(&triple.object) else {
            continue;
        };
        let ranked = match iri.as_str() {
            OWL_DATATYPE => Some((4, PropertyType::DatatypeProperty)),
            OWL_OBJECT => Some((3, PropertyType::ObjectProperty)),
            OWL_ANNOTATION => Some((2, PropertyType::AnnotationProperty)),
            RDF_PROPERTY => Some((1, PropertyType::ObjectProperty)),
            _ => None,
        };
        if let Some(ranked) = ranked
            && best.is_none_or(|(rank, _)| ranked.0 > rank)
        {
            best = Some(ranked);
        }
    }
    best.map(|(_, kind)| kind)
}

fn typed_as(triples: &[Triple], type_iri: &str) -> bool {
    triples.iter().any(|triple| {
        triple.predicate.as_str() == RDF_TYPE
            && term_iri(&triple.object).as_deref() == Some(type_iri)
    })
}

fn term_iri(term: &Term) -> Option<String> {
    match term {
        Term::NamedNode(node) => Some(node.as_str().to_string()),
        _ => None,
    }
}

fn term_text(term: &Term) -> String {
    match term {
        Term::NamedNode(node) => node.as_str().to_string(),
        Term::BlankNode(node) => format!("_:{}", node.as_str()),
        Term::Literal(literal) => literal.value().to_string(),
        #[allow(unreachable_patterns)]
        _ => term.to_string(),
    }
}

fn lang_string(literal: &Literal) -> LangString {
    LangString {
        value: literal.value().to_string(),
        language: literal.language().map(str::to_string),
    }
}

fn preferred_text(values: &[LangString]) -> String {
    crate::model::LangPref::default().prefer(values)
}

fn node_id(node: &NamedOrBlankNode) -> String {
    match node {
        NamedOrBlankNode::NamedNode(node) => node.as_str().to_string(),
        NamedOrBlankNode::BlankNode(node) => format!("_:{}", node.as_str()),
    }
}

fn nonempty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

fn first_non_empty(values: [Option<String>; 4]) -> Option<String> {
    values.into_iter().flatten().next()
}

/// Rebuild an ontology from statements. Used after reasoning adds triples.
pub(crate) fn from_statements(base_uri: &str, format: &str, statements: &[Statement]) -> Ontology {
    let mut triples = Vec::new();
    for statement in statements {
        let Some(triple) = statement_to_triple(statement) else {
            continue;
        };
        triples.push(InputTriple {
            triple,
            graph: statement.graph.clone(),
        });
    }
    let mut ontology = build_ontology(base_uri, format, triples, Vec::new());
    ontology.statements = statements.to_vec();
    ontology
}

pub(crate) fn statement_to_triple(statement: &Statement) -> Option<Triple> {
    let subject = node_from_id(&statement.subject)?;
    let predicate = NamedNode::new(&statement.predicate).ok()?;
    let object = match statement.kind {
        TermKind::Iri => Term::NamedNode(NamedNode::new(&statement.object).ok()?),
        TermKind::Blank => {
            let id = statement
                .object
                .strip_prefix("_:")
                .unwrap_or(&statement.object);
            Term::BlankNode(BlankNode::new(id).ok()?)
        }
        TermKind::Literal => {
            let literal = if let Some(language) = &statement.language {
                Literal::new_language_tagged_literal(&statement.object, language).ok()?
            } else if let Some(datatype) = &statement.datatype {
                Literal::new_typed_literal(&statement.object, NamedNode::new(datatype).ok()?)
            } else {
                Literal::new_simple_literal(&statement.object)
            };
            Term::Literal(literal)
        }
    };
    Some(Triple::new(subject, predicate, object))
}

fn node_from_id(id: &str) -> Option<NamedOrBlankNode> {
    if let Some(blank) = id.strip_prefix("_:") {
        Some(NamedOrBlankNode::BlankNode(BlankNode::new(blank).ok()?))
    } else {
        Some(NamedOrBlankNode::NamedNode(NamedNode::new(id).ok()?))
    }
}

#[allow(dead_code)]
pub(crate) fn meta_type(iri: &str) -> bool {
    matches!(
        iri,
        OWL_CLASS
            | vocab::RDFS_CLASS
            | OWL_OBJECT
            | OWL_DATATYPE
            | OWL_ANNOTATION
            | RDF_PROPERTY
            | OWL_ONTOLOGY
            | OWL_RESTRICTION
            | OWL_FUNCTIONAL
            | OWL_INVERSE_FUNCTIONAL
            | OWL_TRANSITIVE
            | OWL_SYMMETRIC
            | OWL_NAMED_INDIVIDUAL
    )
}
