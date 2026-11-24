//! Capture job rules: what a request must satisfy, and how long a runner keeps a job.

use super::ApiError;
use rupicola_api::{NewJob, TreeSpec};
use rupicola_capture::session::valid_id;
use std::collections::BTreeSet;

/// How long a runner holds a job without reporting, in milliseconds. A runner reports after
/// each capture and while it builds, so a job idle for this long is taken to be abandoned.
pub const LEASE_MS: i64 = 120_000;
/// Trees one job may capture.
const MAX_TREES: usize = 8;
/// Refs one tree may merge, and fixes it may stand for.
const MAX_MERGES: usize = 16;
/// Longest git ref or error message, in characters.
const MAX_TEXT_CHARS: usize = 2000;
/// Longest tree name, in characters.
const MAX_TREE_NAME_CHARS: usize = 64;

/// A git ref a runner will pass to `git`: not empty, not an option, no whitespace or control
/// characters.
fn check_ref(reference: &str) -> Result<(), ApiError> {
    let bad = reference.is_empty()
        || reference.starts_with('-')
        || reference.chars().count() > MAX_TEXT_CHARS
        || reference
            .chars()
            .any(|c| c.is_whitespace() || c.is_control());
    if bad {
        return Err(ApiError::invalid(format!(
            "`{reference}` is not a git ref a runner can check out"
        )));
    }
    Ok(())
}

fn check_tree(tree: &TreeSpec) -> Result<(), ApiError> {
    if !valid_id(&tree.name) || tree.name.chars().count() > MAX_TREE_NAME_CHARS {
        return Err(ApiError::invalid(format!(
            "tree name `{}` must be 1 to {MAX_TREE_NAME_CHARS} letters, digits, `-` or `_`",
            tree.name
        )));
    }
    check_ref(&tree.base)?;
    if tree.merge.len() > MAX_MERGES || tree.fixes.len() > MAX_MERGES {
        return Err(ApiError::invalid(format!(
            "a tree merges and applies at most {MAX_MERGES} refs and fixes"
        )));
    }
    tree.merge
        .iter()
        .try_for_each(|reference| check_ref(reference))
}

/// Checks a new job before it is queued.
///
/// # Errors
///
/// [`ApiError::Invalid`] naming what is out of bounds.
pub fn validate(job: &NewJob) -> Result<(), ApiError> {
    if job.trees.is_empty() || job.trees.len() > MAX_TREES {
        return Err(ApiError::invalid(format!(
            "a job captures 1 to {MAX_TREES} trees"
        )));
    }
    let mut names = BTreeSet::new();
    for tree in &job.trees {
        check_tree(tree)?;
        if !names.insert(tree.name.as_str()) {
            return Err(ApiError::invalid(format!(
                "tree `{}` is named twice",
                tree.name
            )));
        }
    }
    Ok(())
}

/// A runner's error message, bounded.
///
/// # Errors
///
/// [`ApiError::Invalid`] when it is empty or too long.
pub fn check_error(error: &str) -> Result<(), ApiError> {
    if error.trim().is_empty() || error.chars().count() > MAX_TEXT_CHARS {
        return Err(ApiError::invalid(format!(
            "the error must be 1 to {MAX_TEXT_CHARS} characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str, base: &str) -> TreeSpec {
        TreeSpec {
            name: name.to_owned(),
            base: base.to_owned(),
            ..TreeSpec::default()
        }
    }

    fn job(trees: Vec<TreeSpec>) -> NewJob {
        NewJob {
            scenario: "home".to_owned(),
            hash: None,
            trees,
            fix: None,
        }
    }

    #[test]
    fn a_job_needs_one_to_eight_named_trees() {
        assert!(validate(&job(vec![])).is_err());
        assert!(validate(&job(vec![tree("base", "develop")])).is_ok());
        let nine: Vec<_> = (0..=MAX_TREES)
            .map(|n| tree(&format!("t{n}"), "x"))
            .collect();
        assert!(validate(&job(nine)).is_err());
        let twice = vec![tree("base", "a"), tree("base", "b")];
        assert!(validate(&job(twice)).is_err());
        assert!(validate(&job(vec![tree("two words", "a")])).is_err());
    }

    #[test]
    fn a_ref_cannot_be_an_option_or_hold_whitespace() {
        for bad in ["", "--upload-pack=evil", "-b", "a b", "a\nb"] {
            assert!(validate(&job(vec![tree("t", bad)])).is_err(), "{bad:?}");
        }
        let merging = TreeSpec {
            merge: vec!["--evil".to_owned()],
            ..tree("t", "develop")
        };
        assert!(validate(&job(vec![merging])).is_err());
        assert!(validate(&job(vec![tree("t", "feat/x-1")])).is_ok());
    }
}
