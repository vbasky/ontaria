//! Compare two ontology versions and suggest a semver bump.
//!
//! Removing a term, narrowing a range, removing a domain class, raising a
//! minimum cardinality, lowering a maximum, or adding a required restriction
//! is breaking. Adding a term is a minor change. Label-only edits are a patch.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::model::{Class, Ontology, Property, PropertyRef};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SemverBump {
    None,
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TermChange {
    pub kind: ChangeKind,
    pub term: String,
    pub breaking: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyDiff {
    pub changes: Vec<TermChange>,
    pub bump: SemverBump,
}

impl OntologyDiff {
    pub fn has_breaking(&self) -> bool {
        self.changes.iter().any(|change| change.breaking) || self.bump == SemverBump::Major
    }
}

/// Diff `before` against `after`.
pub fn diff(before: &Ontology, after: &Ontology) -> OntologyDiff {
    let mut changes = Vec::new();
    diff_map(
        before.classes.keys(),
        after.classes.keys(),
        "class",
        &mut changes,
        |uri| class_change(before.classes.get(uri), after.classes.get(uri)),
    );
    diff_map(
        before.properties.keys(),
        after.properties.keys(),
        "property",
        &mut changes,
        |uri| property_change(before.properties.get(uri), after.properties.get(uri)),
    );
    diff_map(
        before.individuals.keys(),
        after.individuals.keys(),
        "individual",
        &mut changes,
        |_| None,
    );
    diff_map(
        before.concepts.keys(),
        after.concepts.keys(),
        "concept",
        &mut changes,
        |_| None,
    );
    changes.sort_by(|left, right| {
        left.term
            .cmp(&right.term)
            .then(left.message.cmp(&right.message))
    });
    changes.dedup();
    let bump = bump_for(&changes);
    OntologyDiff { changes, bump }
}

fn diff_map<'a, I>(
    before: I,
    after: I,
    kind: &str,
    changes: &mut Vec<TermChange>,
    changed: impl Fn(&str) -> Option<TermChange>,
) where
    I: Iterator<Item = &'a String>,
{
    let left: BTreeSet<&str> = before.map(String::as_str).collect();
    let right: BTreeSet<&str> = after.map(String::as_str).collect();
    for term in left.difference(&right) {
        changes.push(TermChange {
            kind: ChangeKind::Removed,
            term: (*term).to_string(),
            breaking: true,
            message: format!("removed {kind} {term}"),
        });
    }
    for term in right.difference(&left) {
        changes.push(TermChange {
            kind: ChangeKind::Added,
            term: (*term).to_string(),
            breaking: false,
            message: format!("added {kind} {term}"),
        });
    }
    for term in left.intersection(&right) {
        if let Some(change) = changed(term) {
            changes.push(change);
        }
    }
}

fn class_change(before: Option<&Class>, after: Option<&Class>) -> Option<TermChange> {
    let (before, after) = (before?, after?);
    if before.parents != after.parents && !is_superset(&before.parents, &after.parents) {
        return Some(changed(&after.uri, true, "removed a parent class"));
    }
    if restriction_tightened(&before.properties, &after.properties) {
        return Some(changed(
            &after.uri,
            true,
            "tightened a cardinality restriction",
        ));
    }
    if before.label != after.label || before.comment != after.comment {
        return Some(changed(&after.uri, false, "updated a label or comment"));
    }
    None
}

fn property_change(before: Option<&Property>, after: Option<&Property>) -> Option<TermChange> {
    let (before, after) = (before?, after?);
    if !is_superset(&before.range, &after.range) && before.range != after.range {
        let narrowed = after.range.iter().all(|iri| before.range.contains(iri))
            && after.range.len() < before.range.len()
            || after
                .range
                .iter()
                .any(|iri| !before.range.contains(iri) && !before.range.is_empty());
        if narrowed || after.range.is_empty() && !before.range.is_empty() {
            return Some(changed(&after.uri, true, "narrowed the range"));
        }
    }
    if before.range.iter().any(|iri| !after.range.contains(iri)) && !after.range.is_empty() {
        return Some(changed(&after.uri, true, "removed a range"));
    }
    if before.domain.iter().any(|iri| !after.domain.contains(iri)) {
        return Some(changed(&after.uri, true, "removed a domain class"));
    }
    if before.label != after.label || before.comment != after.comment {
        return Some(changed(&after.uri, false, "updated a label or comment"));
    }
    if before.functional != after.functional && after.functional {
        return Some(changed(&after.uri, true, "became functional"));
    }
    None
}

fn restriction_tightened(before: &[PropertyRef], after: &[PropertyRef]) -> bool {
    for next in after {
        let Some(previous) = before.iter().find(|item| item.uri == next.uri) else {
            return next.required || next.min.unwrap_or(0) > 0 || next.some_values_from.is_some();
        };
        if next.min.unwrap_or(0) > previous.min.unwrap_or(0) {
            return true;
        }
        if let (Some(old), Some(new)) = (previous.max, next.max)
            && new < old
        {
            return true;
        }
        if next.required && !previous.required {
            return true;
        }
    }
    false
}

fn is_superset(before: &[String], after: &[String]) -> bool {
    before.iter().all(|item| after.contains(item))
}

fn changed(term: &str, breaking: bool, message: &str) -> TermChange {
    TermChange {
        kind: ChangeKind::Changed,
        term: term.to_string(),
        breaking,
        message: message.to_string(),
    }
}

fn bump_for(changes: &[TermChange]) -> SemverBump {
    if changes.iter().any(|change| change.breaking) {
        SemverBump::Major
    } else if changes
        .iter()
        .any(|change| change.kind == ChangeKind::Added)
    {
        SemverBump::Minor
    } else if changes.is_empty() {
        SemverBump::None
    } else {
        SemverBump::Patch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document;

    fn parse(ttl: &str) -> Ontology {
        parse_document(ttl.as_bytes(), "http://example.org/").unwrap()
    }

    #[test]
    fn removed_term_is_major_and_label_edit_is_patch() {
        let before = parse(
            "@prefix ex: <http://example.org/> .\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\nex:Person a owl:Class ; rdfs:label \"Person\" .\nex:name a owl:DatatypeProperty ; rdfs:label \"name\" ; rdfs:range <http://www.w3.org/2001/XMLSchema#string> .\n",
        );
        let removed = parse(
            "@prefix ex: <http://example.org/> .\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\nex:Person a owl:Class ; rdfs:label \"Person\" .\n",
        );
        let report = diff(&before, &removed);
        assert_eq!(report.bump, SemverBump::Major);
        assert!(report.has_breaking());

        let relabel = parse(
            "@prefix ex: <http://example.org/> .\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\nex:Person a owl:Class ; rdfs:label \"Human\" .\nex:name a owl:DatatypeProperty ; rdfs:label \"name\" ; rdfs:range <http://www.w3.org/2001/XMLSchema#string> .\n",
        );
        assert_eq!(diff(&before, &relabel).bump, SemverBump::Patch);
    }
}
