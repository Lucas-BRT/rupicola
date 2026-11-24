//! Capture job rules: what a request must satisfy, and how long a runner keeps a job.

use super::ApiError;
use rupicola_api::{NewJob, TreeSpec};
use rupicola_capture::session::valid_id;
use std::collections::BTreeSet;

/// Standard tree names for fix comparison jobs.
pub const TREE_BASE: &str = "base";
pub const TREE_FIX: &str = "fix";
pub const TREE_APPROVED: &str = "approved";
pub const TREE_APPROVED_FIX: &str = "approved-fix";

/// Composes the trees needed to compare a fix (`fix_ref`) against `base_ref` and any already
/// `approved` fixes `(fix_id, git_ref)`.
///
/// When `include_isolated` is true, includes `base` and `fix` (`base + fix_ref`). Whenever
/// `approved` is non-empty, includes `approved` (`base + approved`) and `approved+fix` (`base +
/// approved + fix_ref`).
#[must_use]
pub fn compose_fix_trees(
    base_ref: &str,
    fix_ref: &str,
    approved: &[(String, String)],
    include_isolated: bool,
) -> Vec<TreeSpec> {
    let mut trees = Vec::new();
    if include_isolated {
        trees.push(TreeSpec {
            name: TREE_BASE.to_owned(),
            base: base_ref.to_owned(),
            merge: Vec::new(),
            fixes: Vec::new(),
        });
        trees.push(TreeSpec {
            name: TREE_FIX.to_owned(),
            base: base_ref.to_owned(),
            merge: vec![fix_ref.to_owned()],
            fixes: Vec::new(),
        });
    }
    if !approved.is_empty() {
        let approved_refs: Vec<String> = approved.iter().map(|(_, r)| r.clone()).collect();
        let approved_ids: Vec<String> = approved.iter().map(|(id, _)| id.clone()).collect();
        trees.push(TreeSpec {
            name: TREE_APPROVED.to_owned(),
            base: base_ref.to_owned(),
            merge: approved_refs.clone(),
            fixes: approved_ids.clone(),
        });
        let mut with_fix = approved_refs;
        with_fix.push(fix_ref.to_owned());
        trees.push(TreeSpec {
            name: TREE_APPROVED_FIX.to_owned(),
            base: base_ref.to_owned(),
            merge: with_fix,
            fixes: approved_ids,
        });
    }
    trees
}

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

    #[test]
    fn compose_fix_trees_builds_isolated_and_integrated_trees() {
        let isolated_only = compose_fix_trees("develop", "fix/b", &[], true);
        assert_eq!(isolated_only.len(), 2);
        assert_eq!(isolated_only[0].name, TREE_BASE);
        assert_eq!(isolated_only[1].name, TREE_FIX);
        assert_eq!(isolated_only[1].merge, vec!["fix/b"]);

        let approved = vec![("fix-a".to_owned(), "fix/a".to_owned())];
        let both = compose_fix_trees("develop", "fix/b", &approved, true);
        assert_eq!(both.len(), 4);
        assert_eq!(both[2].name, TREE_APPROVED);
        assert_eq!(both[2].merge, vec!["fix/a"]);
        assert_eq!(both[2].fixes, vec!["fix-a"]);
        assert_eq!(both[3].name, TREE_APPROVED_FIX);
        assert_eq!(both[3].merge, vec!["fix/a", "fix/b"]);
        assert_eq!(both[3].fixes, vec!["fix-a"]);

        let integrated_only = compose_fix_trees("develop", "fix/b", &approved, false);
        assert_eq!(integrated_only.len(), 2);
        assert_eq!(integrated_only[0].name, TREE_APPROVED);
        assert_eq!(integrated_only[1].name, TREE_APPROVED_FIX);
    }
}
