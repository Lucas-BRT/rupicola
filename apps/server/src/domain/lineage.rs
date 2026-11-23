//! The rules of a capture's lineage: how far a graph may reach, what an edit of the parents
//! must say, and what changed between two captures.

use super::ApiError;
use rupicola_api::{CaptureDiff, ChangedFile, FacetChange, ParentsEdit};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// Longest reason an edit may give, in characters.
const MAX_REASON_CHARS: usize = 500;
/// How far a lineage graph reaches either way unless asked otherwise, and the farthest it may.
const DEFAULT_DEPTH: u32 = 5;
const MAX_DEPTH: u32 = 20;

/// The depth a graph reaches: `asked`, or the default.
pub fn depth(asked: Option<u32>) -> Result<u32, ApiError> {
    let depth = asked.unwrap_or(DEFAULT_DEPTH);
    if (1..=MAX_DEPTH).contains(&depth) {
        Ok(depth)
    } else {
        Err(ApiError::invalid(format!("depth must be 1 to {MAX_DEPTH}")))
    }
}

/// Checks what an edit of `capture`'s parents says on its own; returns its trimmed reason.
pub fn check_edit<'a>(capture: &str, form: &'a ParentsEdit) -> Result<&'a str, ApiError> {
    let reason = form.reason.trim();
    if !(1..=MAX_REASON_CHARS).contains(&reason.chars().count()) {
        return Err(ApiError::invalid(format!(
            "say why the lineage changes, in at most {MAX_REASON_CHARS} characters"
        )));
    }
    if form.parents.iter().any(|parent| parent.id == capture) {
        return Err(ApiError::invalid("a capture cannot be its own parent"));
    }
    let mut seen = HashSet::new();
    if form
        .parents
        .iter()
        .any(|parent| !seen.insert(parent.id.as_str()))
    {
        return Err(ApiError::invalid("a parent is listed twice"));
    }
    Ok(reason)
}

/// Fails unless every one of `ids` is among the project's live captures `found`.
pub fn check_known(ids: &[&str], found: &[String]) -> Result<(), ApiError> {
    let absent: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| !found.iter().any(|known| known == id))
        .collect();
    if absent.is_empty() {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "these are not captures of the project: {}",
            absent.join(", ")
        )))
    }
}

/// The refusal for a parent that descends from the capture it would parent.
pub fn cycle(parent: &str) -> ApiError {
    ApiError::invalid(format!(
        "{parent} descends from this capture, so it cannot be its parent"
    ))
}

/// What a capture holds, for comparing: its files by path and its facets by key.
pub struct Contents {
    pub files: BTreeMap<String, String>,
    pub facets: BTreeMap<String, String>,
}

/// What changed from `before` to `after`: files matched by path, and facets.
pub fn diff(before: (String, Contents), after: (String, Contents)) -> CaptureDiff {
    let (before, old) = before;
    let (after, new) = after;
    let mut diff = CaptureDiff {
        before,
        after,
        same: Vec::new(),
        changed: Vec::new(),
        added: new
            .files
            .keys()
            .filter(|path| !old.files.contains_key(*path))
            .cloned()
            .collect(),
        removed: Vec::new(),
        facets: Vec::new(),
    };
    for (path, sha) in &old.files {
        match new.files.get(path) {
            Some(other) if other == sha => diff.same.push(path.clone()),
            Some(other) => diff.changed.push(ChangedFile {
                path: path.clone(),
                before: sha.clone(),
                after: other.clone(),
            }),
            None => diff.removed.push(path.clone()),
        }
    }
    let keys: BTreeSet<&String> = old.facets.keys().chain(new.facets.keys()).collect();
    diff.facets = keys
        .into_iter()
        .filter(|key| old.facets.get(*key) != new.facets.get(*key))
        .map(|key| FacetChange {
            key: key.clone(),
            before: old.facets.get(key).cloned(),
            after: new.facets.get(key).cloned(),
        })
        .collect();
    diff
}
