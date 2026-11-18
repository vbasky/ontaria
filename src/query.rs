//! Lookups over one parsed ontology.
//!
//! A term resolves in four steps: full IRI, `prefix:Name`, local name
//! (`#Name` or `/Name`), then a case-insensitive label.

use serde::{Deserialize, Serialize};

use crate::model::{
    Annotation, Class, Concept, ConceptScheme, Individual, LangString, Ontology, Property,
    PropertyType,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResults {
    pub classes: Vec<Class>,
    pub properties: Vec<Property>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concepts: Vec<Concept>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub individuals: Vec<Individual>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropertyDetails {
    pub property: Property,
    pub source_class: String,
    pub source_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_details: Option<Class>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassDetails {
    pub class: Class,
    pub direct_properties: Vec<PropertyDetails>,
    pub inherited_properties: Vec<PropertyDetails>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropertyUsage {
    pub property: Property,
    pub domain_classes: Vec<Class>,
    pub range_classes: Vec<Class>,
    pub suggested_classes: Vec<Class>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassPropertiesResult {
    pub class: Class,
    pub hierarchy: Vec<Class>,
    pub properties_of_class: Vec<PropertyDetails>,
    pub properties_referencing_class: Vec<PropertyDetails>,
    pub inherited_properties: Vec<PropertyDetails>,
}

impl Ontology {
    pub fn get_class(&self, term: &str) -> Option<&Class> {
        if let Some(class) = self.classes.get(term) {
            return Some(class);
        }
        let resolved = self.resolve_prefix(term);
        if let Some(class) = self.classes.get(&resolved) {
            return Some(class);
        }
        let hash = format!("#{term}");
        let slash = format!("/{term}");
        if let Some(class) = self
            .classes
            .values()
            .find(|class| class.uri.ends_with(&hash) || class.uri.ends_with(&slash))
        {
            return Some(class);
        }
        let lower = term.to_lowercase();
        self.classes
            .values()
            .find(|class| class.label.to_lowercase() == lower)
    }

    pub fn get_property(&self, term: &str) -> Option<&Property> {
        if let Some(property) = self.properties.get(term) {
            return Some(property);
        }
        let resolved = self.resolve_prefix(term);
        if let Some(property) = self.properties.get(&resolved) {
            return Some(property);
        }
        let hash = format!("#{term}");
        let slash = format!("/{term}");
        if let Some(property) = self
            .properties
            .values()
            .find(|property| property.uri.ends_with(&hash) || property.uri.ends_with(&slash))
        {
            return Some(property);
        }
        let lower = term.to_lowercase();
        self.properties
            .values()
            .find(|property| label_is(&property.labels, &property.label, &lower))
    }

    pub fn get_individual(&self, term: &str) -> Option<&Individual> {
        resolve(
            &self.individuals,
            term,
            self,
            |item| item.uri.as_str(),
            |item| item.label.as_str(),
        )
    }

    pub fn get_concept(&self, term: &str) -> Option<&Concept> {
        resolve(
            &self.concepts,
            term,
            self,
            |item| item.uri.as_str(),
            |item| item.pref_label.as_str(),
        )
    }

    pub fn get_scheme(&self, term: &str) -> Option<&ConceptScheme> {
        resolve(
            &self.schemes,
            term,
            self,
            |item| item.uri.as_str(),
            |item| item.label.as_str(),
        )
    }

    /// Annotations recorded on a class, property, individual, or concept.
    pub fn annotations_of(&self, term: &str) -> Vec<&Annotation> {
        if let Some(class) = self.get_class(term) {
            return class.annotations.iter().collect();
        }
        if let Some(property) = self.get_property(term) {
            return property.annotations.iter().collect();
        }
        if let Some(individual) = self.get_individual(term) {
            return individual.annotations.iter().collect();
        }
        if let Some(concept) = self.get_concept(term) {
            return concept.annotations.iter().collect();
        }
        Vec::new()
    }

    /// Direct super-properties. `None` when the property does not exist.
    pub fn get_property_parents(&self, term: &str) -> Option<Vec<&Property>> {
        let property = self.get_property(term)?;
        Some(
            property
                .super_properties
                .iter()
                .filter_map(|uri| self.properties.get(uri))
                .collect(),
        )
    }

    pub fn get_property_children(&self, term: &str) -> Option<Vec<&Property>> {
        let property = self.get_property(term)?;
        Some(
            property
                .sub_properties
                .iter()
                .filter_map(|uri| self.properties.get(uri))
                .collect(),
        )
    }

    pub fn list_classes(&self, query: &str) -> Vec<&Class> {
        let filter = query.to_lowercase();
        self.classes
            .values()
            .filter(|class| {
                filter.is_empty()
                    || class.label.to_lowercase().contains(&filter)
                    || class.uri.to_lowercase().contains(&filter)
            })
            .collect()
    }

    pub fn list_properties(&self, term: &str) -> Vec<&Property> {
        if term.is_empty() {
            return self.properties.values().collect();
        }
        let Some(class) = self.get_class(term) else {
            return Vec::new();
        };
        self.properties
            .values()
            .filter(|property| property.domain.iter().any(|domain| domain == &class.uri))
            .collect()
    }

    /// `None` when the class does not exist. An empty vector means it has no
    /// parents that are themselves declared classes.
    pub fn get_parents(&self, term: &str) -> Option<Vec<&Class>> {
        let class = self.get_class(term)?;
        Some(
            class
                .parents
                .iter()
                .filter_map(|uri| self.classes.get(uri))
                .collect(),
        )
    }

    pub fn get_children(&self, term: &str) -> Option<Vec<&Class>> {
        let class = self.get_class(term)?;
        Some(
            class
                .children
                .iter()
                .filter_map(|uri| self.classes.get(uri))
                .collect(),
        )
    }

    pub fn search(&self, query: &str) -> SearchResults {
        let keyword = query.to_lowercase();
        let mut classes: Vec<Class> = self
            .classes
            .values()
            .filter(|class| {
                matches_keyword(
                    &class.label,
                    &class.comment,
                    &class.description,
                    &class.uri,
                    &keyword,
                ) || texts_match(&class.labels, &keyword)
                    || texts_match(&class.comments, &keyword)
            })
            .cloned()
            .collect();
        let mut properties: Vec<Property> = self
            .properties
            .values()
            .filter(|property| {
                matches_keyword(
                    &property.label,
                    &property.comment,
                    &property.description,
                    &property.uri,
                    &keyword,
                ) || texts_match(&property.labels, &keyword)
                    || texts_match(&property.comments, &keyword)
            })
            .cloned()
            .collect();
        let mut concepts: Vec<Concept> = self
            .concepts
            .values()
            .filter(|concept| {
                concept.pref_label.to_lowercase().contains(&keyword)
                    || concept.uri.to_lowercase().contains(&keyword)
                    || texts_match(&concept.pref_labels, &keyword)
                    || texts_match(&concept.alt_labels, &keyword)
            })
            .cloned()
            .collect();
        let mut individuals: Vec<Individual> = self
            .individuals
            .values()
            .filter(|individual| {
                individual.label.to_lowercase().contains(&keyword)
                    || individual.uri.to_lowercase().contains(&keyword)
                    || texts_match(&individual.labels, &keyword)
            })
            .cloned()
            .collect();
        classes.sort_by(|left, right| left.uri.cmp(&right.uri));
        properties.sort_by(|left, right| left.uri.cmp(&right.uri));
        concepts.sort_by(|left, right| left.uri.cmp(&right.uri));
        individuals.sort_by(|left, right| left.uri.cmp(&right.uri));
        SearchResults {
            classes,
            properties,
            concepts,
            individuals,
        }
    }

    pub fn get_class_with_inherited_properties(&self, term: &str) -> Option<ClassDetails> {
        let class = self.get_class(term)?.clone();
        let mut details = ClassDetails {
            direct_properties: self.properties_for_domain(&class.uri, &class.label),
            class,
            inherited_properties: Vec::new(),
        };
        self.add_restriction_properties(&details.class, &mut details.direct_properties);
        let mut visited = Vec::new();
        self.collect_inherited(
            &details.class.uri,
            &mut details.inherited_properties,
            &mut visited,
        );
        Some(details)
    }

    pub fn get_property_with_usage(&self, term: &str) -> Option<PropertyUsage> {
        let property = self.get_property(term)?.clone();
        let domain_classes = property
            .domain
            .iter()
            .filter_map(|uri| self.classes.get(uri).cloned())
            .collect::<Vec<_>>();
        let range_classes = property
            .range
            .iter()
            .filter_map(|uri| self.classes.get(uri).cloned())
            .collect::<Vec<_>>();
        let (note, suggested_classes) = if domain_classes.is_empty() {
            (
                "This property has no explicit domain defined. Below are suggested classes based on naming similarity.".to_string(),
                self.find_related_classes(&property),
            )
        } else {
            (String::new(), Vec::new())
        };
        Some(PropertyUsage {
            property,
            domain_classes,
            range_classes,
            suggested_classes,
            note,
        })
    }

    pub fn get_class_properties(&self, term: &str) -> Option<ClassPropertiesResult> {
        let class = self.get_class(term)?.clone();
        let properties_of_class = self.properties_for_domain(&class.uri, &class.label);
        let mut properties_referencing_class = Vec::new();
        for property in self.properties.values() {
            if !property.range.iter().any(|range| range == &class.uri) {
                continue;
            }
            let (source_class, source_uri) = property
                .domain
                .first()
                .and_then(|uri| {
                    self.classes
                        .get(uri)
                        .map(|domain| (domain.label.clone(), uri.clone()))
                })
                .unwrap_or_default();
            properties_referencing_class.push(self.detail(property, source_class, source_uri));
        }
        let mut inherited_properties = Vec::new();
        let mut visited = Vec::new();
        self.collect_inherited(&class.uri, &mut inherited_properties, &mut visited);
        Some(ClassPropertiesResult {
            hierarchy: self.build_hierarchy(&class),
            class,
            properties_of_class,
            properties_referencing_class,
            inherited_properties,
        })
    }

    fn properties_for_domain(&self, class_uri: &str, label: &str) -> Vec<PropertyDetails> {
        self.properties
            .values()
            .filter(|property| property.domain.iter().any(|domain| domain == class_uri))
            .map(|property| self.detail(property, label.to_string(), class_uri.to_string()))
            .collect()
    }

    fn add_restriction_properties(&self, class: &Class, into: &mut Vec<PropertyDetails>) {
        for property_ref in &class.properties {
            if into
                .iter()
                .any(|existing| existing.property.uri == property_ref.uri)
            {
                continue;
            }
            let Some(property) = self.properties.get(&property_ref.uri) else {
                continue;
            };
            into.push(self.detail(property, class.label.clone(), class.uri.clone()));
        }
    }

    fn collect_inherited(
        &self,
        class_uri: &str,
        into: &mut Vec<PropertyDetails>,
        visited: &mut Vec<String>,
    ) {
        let Some(class) = self.classes.get(class_uri) else {
            return;
        };
        let parents = class.parents.clone();
        for parent_uri in parents {
            if visited.iter().any(|seen| seen == &parent_uri) {
                continue;
            }
            visited.push(parent_uri.clone());
            let Some(parent) = self.classes.get(&parent_uri) else {
                continue;
            };
            let label = parent.label.clone();
            for property in self.properties.values() {
                if !property.domain.iter().any(|domain| domain == &parent_uri) {
                    continue;
                }
                if into
                    .iter()
                    .any(|existing| existing.property.uri == property.uri)
                {
                    continue;
                }
                into.push(self.detail(property, label.clone(), parent_uri.clone()));
            }
            let restriction_refs = parent.properties.clone();
            for property_ref in restriction_refs {
                if into
                    .iter()
                    .any(|existing| existing.property.uri == property_ref.uri)
                {
                    continue;
                }
                let Some(property) = self.properties.get(&property_ref.uri) else {
                    continue;
                };
                into.push(self.detail(property, label.clone(), parent_uri.clone()));
            }
            self.collect_inherited(&parent_uri, into, visited);
        }
    }

    fn detail(
        &self,
        property: &Property,
        source_class: String,
        source_uri: String,
    ) -> PropertyDetails {
        let range_details = if property.kind == PropertyType::ObjectProperty {
            property
                .range
                .first()
                .and_then(|uri| self.classes.get(uri).cloned())
        } else {
            None
        };
        PropertyDetails {
            property: property.clone(),
            source_class,
            source_uri,
            range_details,
        }
    }

    /// Root-first chain through the first parent. Cycles stop at the repeated class.
    fn build_hierarchy(&self, class: &Class) -> Vec<Class> {
        let mut chain = Vec::new();
        let mut cursor = Some(class.uri.as_str());
        let mut guard = 0;
        while let Some(uri) = cursor {
            if chain.iter().any(|seen: &Class| seen.uri == uri) {
                break;
            }
            let Some(current) = self.classes.get(uri) else {
                break;
            };
            chain.push(current.clone());
            cursor = current.parents.first().map(String::as_str);
            guard += 1;
            if guard > 10_000 {
                break;
            }
        }
        chain.reverse();
        chain
    }

    fn find_related_classes(&self, property: &Property) -> Vec<Class> {
        let name = if !property.label.is_empty() {
            property.label.clone()
        } else {
            local_name(&property.uri)
        };
        let keywords = extract_keywords(&name);
        let mut matches: Vec<Class> = self
            .classes
            .values()
            .filter(|class| {
                let label = class.label.to_lowercase();
                let uri = class.uri.to_lowercase();
                keywords
                    .iter()
                    .any(|keyword| label.contains(keyword) || uri.contains(keyword.as_str()))
            })
            .cloned()
            .collect();
        matches.sort_by(|left, right| left.uri.cmp(&right.uri));
        matches.truncate(5);
        matches
    }
}

fn resolve<'a, T>(
    map: &'a std::collections::BTreeMap<String, T>,
    term: &str,
    ontology: &Ontology,
    uri: impl Fn(&T) -> &str,
    label: impl Fn(&T) -> &str,
) -> Option<&'a T> {
    if let Some(found) = map.get(term) {
        return Some(found);
    }
    let resolved = ontology.resolve_prefix(term);
    if let Some(found) = map.get(&resolved) {
        return Some(found);
    }
    let hash = format!("#{term}");
    let slash = format!("/{term}");
    if let Some(found) = map
        .values()
        .find(|item| uri(item).ends_with(&hash) || uri(item).ends_with(&slash))
    {
        return Some(found);
    }
    let lower = term.to_lowercase();
    map.values()
        .find(|item| label(item).to_lowercase() == lower)
}

fn label_is(labels: &[LangString], preferred: &str, lower: &str) -> bool {
    preferred.to_lowercase() == lower
        || labels.iter().any(|item| item.value.to_lowercase() == lower)
}

fn texts_match(values: &[LangString], keyword: &str) -> bool {
    values
        .iter()
        .any(|item| item.value.to_lowercase().contains(keyword))
}

fn matches_keyword(
    label: &str,
    comment: &str,
    description: &str,
    uri: &str,
    keyword: &str,
) -> bool {
    label.to_lowercase().contains(keyword)
        || comment.to_lowercase().contains(keyword)
        || description.to_lowercase().contains(keyword)
        || uri.to_lowercase().contains(keyword)
}

fn local_name(uri: &str) -> String {
    uri.rsplit(['#', '/']).next().unwrap_or(uri).to_string()
}

/// Split a camelCase or spaced property name into lowercase words of 3+ characters.
/// A leading `has` or `is` is dropped first.
pub fn extract_keywords(name: &str) -> Vec<String> {
    let lower = name.to_lowercase();
    let rest = if lower.starts_with("has") {
        &name[3..]
    } else if lower.starts_with("is") {
        &name[2..]
    } else {
        name
    };
    let words: Vec<String> = if rest.contains(' ') {
        rest.split_whitespace().map(str::to_string).collect()
    } else {
        let mut words = Vec::new();
        let mut current = String::new();
        for (index, ch) in rest.chars().enumerate() {
            if index > 0 && ch.is_ascii_uppercase() && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(ch);
        }
        if !current.is_empty() {
            words.push(current);
        }
        words
    };
    words
        .into_iter()
        .map(|word| word.to_lowercase())
        .filter(|word| word.chars().count() >= 3)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_turtle;

    fn sample() -> Ontology {
        parse_turtle(
            include_str!("../testdata/simple.ttl").as_bytes(),
            "http://example.org/test",
        )
        .unwrap()
    }

    #[test]
    fn resolves_class_by_iri_prefix_local_name_and_label() {
        let ontology = sample();
        assert_eq!(
            ontology
                .get_class("http://example.org/test#Person")
                .unwrap()
                .label,
            "Person"
        );
        assert_eq!(ontology.get_class("ex:Person").unwrap().label, "Person");
        assert_eq!(ontology.get_class("Person").unwrap().label, "Person");
        assert_eq!(ontology.get_class("person").unwrap().label, "Person");
        assert!(ontology.get_class("Missing").is_none());
    }

    #[test]
    fn inherited_properties_come_from_the_parent_domain() {
        let ontology = sample();
        let details = ontology
            .get_class_with_inherited_properties("Person")
            .unwrap();
        let direct: Vec<_> = details
            .direct_properties
            .iter()
            .map(|item| item.property.label.as_str())
            .collect();
        assert!(direct.contains(&"age"));
        assert!(direct.contains(&"knows"));
        let inherited: Vec<_> = details
            .inherited_properties
            .iter()
            .map(|item| item.property.label.as_str())
            .collect();
        assert_eq!(inherited, vec!["name"]);
        assert_eq!(
            details
                .direct_properties
                .iter()
                .find(|item| item.property.label == "knows")
                .unwrap()
                .range_details
                .as_ref()
                .unwrap()
                .label,
            "Person"
        );
    }

    #[test]
    fn parents_children_and_search() {
        let ontology = sample();
        let parents = ontology.get_parents("Person").unwrap();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].label, "Agent");
        let children = ontology.get_children("Agent").unwrap();
        assert_eq!(children[0].label, "Person");
        assert!(ontology.get_parents("Nope").is_none());
        let found = ontology.search("human");
        assert_eq!(found.classes.len(), 1);
        assert_eq!(found.classes[0].label, "Person");
    }

    #[test]
    fn property_usage_suggests_classes_when_domain_is_missing() {
        let ontology = sample();
        let usage = ontology.get_property_with_usage("nickname").unwrap();
        assert!(usage.domain_classes.is_empty());
        assert!(!usage.note.is_empty());
        assert!(
            usage
                .suggested_classes
                .iter()
                .any(|class| class.label == "Person")
        );
    }

    #[test]
    fn class_properties_include_incoming_range() {
        let ontology = sample();
        let result = ontology.get_class_properties("Person").unwrap();
        assert!(result.hierarchy.iter().any(|class| class.label == "Agent"));
        assert!(
            result
                .properties_referencing_class
                .iter()
                .any(|item| item.property.label == "knows")
        );
    }

    #[test]
    fn keywords_split_camel_case() {
        assert_eq!(
            extract_keywords("videoBitRate"),
            vec!["video".to_string(), "bit".to_string(), "rate".to_string()]
        );
        assert_eq!(
            extract_keywords("hasAudioTrack"),
            vec!["audio".to_string(), "track".to_string()]
        );
    }
}
