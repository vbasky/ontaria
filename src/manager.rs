//! Search across `owl:imports`.
//!
//! Imports are fetched into the shared cache the first time a query asks for
//! them, then parsed and remembered for the rest of the process. A cycle is
//! reported as a warning and does not recurse again.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::cache::CacheManager;
use crate::model::{Class, Ontology, Property};
use crate::parser::parse_document;
use crate::query::SearchResults;

#[cfg_attr(not(test), allow(dead_code))]
pub struct OntologyManager {
    main: Ontology,
    loaded: HashMap<String, Ontology>,
    cache: CacheManager,
    warnings: Vec<String>,
}

#[allow(dead_code)]
impl OntologyManager {
    pub fn new(main: Ontology, cache: CacheManager) -> Self {
        Self {
            main,
            loaded: HashMap::new(),
            cache,
            warnings: Vec::new(),
        }
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn main(&self) -> &Ontology {
        &self.main
    }

    /// One ontology containing the main file and every import it reaches.
    /// A class declared here can see parents and properties declared in an import.
    pub fn merged(&mut self) -> Ontology {
        self.ensure_imports();
        let imported = self
            .loaded_in_order()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut combined = self.main.clone();
        for ontology in imported {
            combined.absorb(&ontology);
        }
        combined
    }

    pub fn get_class(&mut self, term: &str, imports: bool) -> Option<&Class> {
        if self.main.get_class(term).is_some() {
            return self.main.get_class(term);
        }
        if !imports {
            return None;
        }
        self.ontology_with_class(term)
            .and_then(|ontology| ontology.get_class(term))
    }

    pub fn get_property(&mut self, term: &str, imports: bool) -> Option<&Property> {
        if self.main.get_property(term).is_some() {
            return self.main.get_property(term);
        }
        if !imports {
            return None;
        }
        self.ontology_with_property(term)
            .and_then(|ontology| ontology.get_property(term))
    }

    /// The ontology that declares `term`, searching imports when it is not in
    /// the main file.
    pub fn ontology_with_class(&mut self, term: &str) -> Option<&Ontology> {
        if self.main.get_class(term).is_some() {
            return Some(&self.main);
        }
        self.ensure_imports();
        self.find_loaded_class(term)
    }

    pub fn ontology_with_property(&mut self, term: &str) -> Option<&Ontology> {
        if self.main.get_property(term).is_some() {
            return Some(&self.main);
        }
        self.ensure_imports();
        self.find_loaded_property(term)
    }

    pub fn search(&mut self, query: &str, imports: bool) -> SearchResults {
        let mut results = self.main.search(query);
        if imports {
            self.ensure_imports();
            let extras: Vec<SearchResults> = self
                .loaded_in_order()
                .into_iter()
                .map(|ontology| ontology.search(query))
                .collect();
            for found in extras {
                results.classes.extend(found.classes);
                results.properties.extend(found.properties);
                results.concepts.extend(found.concepts);
                results.individuals.extend(found.individuals);
            }
        }
        results
    }

    pub fn batch_classes(&mut self, terms: &[String], imports: bool) -> Map<String, Value> {
        let mut map = Map::new();
        for term in terms {
            let value = match self.get_class(term, imports).cloned() {
                Some(class) => serde_json::to_value(class).unwrap_or(Value::Null),
                None => Value::Null,
            };
            map.insert(term.clone(), value);
        }
        map
    }

    pub fn batch_properties(&mut self, terms: &[String], imports: bool) -> Map<String, Value> {
        let mut map = Map::new();
        for term in terms {
            let value = match self.get_property(term, imports).cloned() {
                Some(property) => serde_json::to_value(property).unwrap_or(Value::Null),
                None => Value::Null,
            };
            map.insert(term.clone(), value);
        }
        map
    }

    fn ensure_imports(&mut self) {
        let imports = self.main.imports.clone();
        let mut visited = HashSet::new();
        for uri in imports {
            let _ = self.load_recursive(&uri, &mut visited);
        }
    }

    fn loaded_in_order(&self) -> Vec<&Ontology> {
        let mut ordered = Vec::new();
        let mut pending = self.main.imports.clone();
        let mut seen = HashSet::new();
        let mut index = 0;
        while index < pending.len() {
            let uri = pending[index].clone();
            index += 1;
            if !seen.insert(uri.clone()) {
                continue;
            }
            if let Some(ontology) = self.loaded.get(&uri) {
                pending.extend(ontology.imports.iter().cloned());
                ordered.push(ontology);
            }
        }
        ordered
    }

    fn find_loaded_class(&self, name: &str) -> Option<&Ontology> {
        self.loaded_in_order()
            .into_iter()
            .find(|ontology| ontology.get_class(name).is_some())
    }

    fn find_loaded_property(&self, name: &str) -> Option<&Ontology> {
        self.loaded_in_order()
            .into_iter()
            .find(|ontology| ontology.get_property(name).is_some())
    }

    fn load_recursive(&mut self, uri: &str, visited: &mut HashSet<String>) -> Option<()> {
        if !visited.insert(uri.to_string()) {
            self.warnings
                .push(format!("Circular import detected: {uri}"));
            return None;
        }
        if self.loaded.contains_key(uri) {
            let nested = self
                .loaded
                .get(uri)
                .map(|ontology| ontology.imports.clone())?;
            for child in nested {
                let _ = self.load_recursive(&child, visited);
            }
            return Some(());
        }
        if !self.cache.exists(uri)
            && let Err(err) = self.cache.fetch(uri)
        {
            self.warnings
                .push(format!("Failed to fetch import {uri}: {err}"));
            return None;
        }
        let bytes = match self.cache.load(uri) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.warnings
                    .push(format!("Failed to load import {uri} from cache: {err}"));
                return None;
            }
        };
        let ontology = match parse_document(bytes.as_slice(), uri) {
            Ok(ontology) => ontology,
            Err(err) => {
                self.warnings
                    .push(format!("Failed to parse import {uri}: {err}"));
                return None;
            }
        };
        let nested = ontology.imports.clone();
        self.loaded.insert(uri.to_string(), ontology);
        for child in nested {
            let _ = self.load_recursive(&child, visited);
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_document, parse_turtle};
    use std::fs;

    #[test]
    fn finds_a_class_through_an_import_and_stops_on_a_cycle() {
        let dir = std::env::temp_dir().join(format!("ontaria-imports-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = CacheManager::open(&dir).unwrap();
        let main_url = "http://example.org/main";
        let other_url = "http://example.org/other";
        cache
            .save(
                main_url,
                b"
                @prefix owl: <http://www.w3.org/2002/07/owl#> .
                @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
                <http://example.org/main> a owl:Ontology ;
                    owl:imports <http://example.org/other> .
                <http://example.org/main#Local> a owl:Class ; rdfs:label \"Local\" .
                ",
            )
            .unwrap();
        cache
            .save(
                other_url,
                b"
                @prefix owl: <http://www.w3.org/2002/07/owl#> .
                @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
                <http://example.org/other> a owl:Ontology ;
                    owl:imports <http://example.org/main> .
                <http://example.org/other#Remote> a owl:Class ; rdfs:label \"Remote\" .
                ",
            )
            .unwrap();
        let bytes = cache.load(main_url).unwrap();
        let ontology = parse_turtle(bytes.as_slice(), main_url).unwrap();
        let mut manager = OntologyManager::new(ontology, cache);
        assert!(manager.get_class("Remote", false).is_none());
        assert_eq!(manager.get_class("Remote", true).unwrap().label, "Remote");
        assert!(
            manager
                .warnings()
                .iter()
                .any(|warning| warning.contains("Circular import"))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn merged_import_exposes_the_parent_and_its_property() {
        let dir = std::env::temp_dir().join(format!("ontaria-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cache = CacheManager::open(&dir).unwrap();
        let main_url = "http://example.org/news";
        let other_url = "http://example.org/work";
        cache
            .save(
                main_url,
                b"
                @prefix owl: <http://www.w3.org/2002/07/owl#> .
                @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
                <http://example.org/news> a owl:Ontology ;
                    owl:imports <http://example.org/work> .
                <http://example.org/news#NewsItem> a owl:Class ;
                    rdfs:label \"News item\" ;
                    rdfs:subClassOf <http://example.org/work#Work> .
                ",
            )
            .unwrap();
        cache
            .save(
                other_url,
                br#"
                @prefix owl: <http://www.w3.org/2002/07/owl#> .
                @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
                @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
                <http://example.org/work#Work> a owl:Class ; rdfs:label "Work" .
                <http://example.org/work#title> a owl:DatatypeProperty ;
                    rdfs:label "title" ;
                    rdfs:domain <http://example.org/work#Work> ;
                    rdfs:range xsd:string .
                "#,
            )
            .unwrap();
        let bytes = cache.load(main_url).unwrap();
        let ontology = parse_document(bytes.as_slice(), main_url).unwrap();
        let mut manager = OntologyManager::new(ontology, cache);
        let merged = manager.merged();
        let parents = merged.get_parents("NewsItem").unwrap();
        assert_eq!(parents[0].label, "Work");
        let details = merged
            .get_class_with_inherited_properties("NewsItem")
            .unwrap();
        assert!(
            details
                .inherited_properties
                .iter()
                .any(|item| item.property.label == "title")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
