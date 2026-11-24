//! Fix domain rules: field bounds for filing, steps and reviews.

use crate::domain::{ApiError, problem::check_len};
use rupicola_api::{
    Comparison, ComparisonView, FixStatus, NewFix, NewReview, NewStep, ProblemStatus,
};

/// Longest fix title or comparison label, in characters.
const MAX_TITLE_CHARS: usize = 200;
/// Longest summary, step detail, verification or review comment, in characters.
const MAX_TEXT_CHARS: usize = 8000;
/// Problems, tactics, files, comparisons or steps one fix may hold of each kind.
const MAX_ITEMS: usize = 64;
/// Label of the selection a fix's comparisons make of their before files, and of their after
/// and diff files.
pub const BEFORE_LABEL: &str = "Before";
pub const AFTER_LABEL: &str = "After";

/// Longest revision name a comparison holds, in characters.
const MAX_REVISION_CHARS: usize = 200;

/// Validates an optional git ref (branch or commit) attached to a fix.
pub fn check_git_ref(reference: &str) -> Result<(), ApiError> {
    let trimmed = reference.trim();
    let bad = trimmed.is_empty()
        || trimmed.starts_with('-')
        || trimmed.chars().count() > MAX_REVISION_CHARS
        || trimmed.chars().any(|c| c.is_whitespace() || c.is_control());
    if bad {
        return Err(ApiError::invalid(format!(
            "`{reference}` is not a valid git ref"
        )));
    }
    Ok(())
}

/// Field bounds and view rules of a new comparison.
fn check_comparison(comparison: &Comparison) -> Result<(), ApiError> {
    check_len(
        "a comparison label",
        &comparison.label,
        MAX_TITLE_CHARS,
        true,
    )?;
    check_len(
        "a comparison base revision",
        &comparison.base_revision,
        MAX_REVISION_CHARS,
        false,
    )?;
    check_len(
        "a comparison revision",
        &comparison.revision,
        MAX_REVISION_CHARS,
        false,
    )?;
    if comparison.applied_fixes.len() > MAX_ITEMS {
        return Err(ApiError::invalid(format!(
            "a comparison lists at most {MAX_ITEMS} applied fixes"
        )));
    }
    if comparison.view == ComparisonView::Isolated && !comparison.applied_fixes.is_empty() {
        return Err(ApiError::invalid(
            "an isolated comparison applies no other fix",
        ));
    }
    Ok(())
}

/// Field bounds of a new step.
pub fn check_step(step: &NewStep) -> Result<(), ApiError> {
    check_len("a step title", step.title.trim(), MAX_TITLE_CHARS, true)?;
    check_len("a step detail", &step.detail, MAX_TEXT_CHARS, false)?;
    check_len(
        "a step verification",
        &step.verification,
        MAX_TEXT_CHARS,
        false,
    )
}

/// Field bounds of a new fix.
///
/// # Errors
///
/// [`ApiError::Invalid`] when any field or collection is empty/oversized.
pub fn validate(form: &NewFix) -> Result<(), ApiError> {
    check_len("the title", form.title.trim(), MAX_TITLE_CHARS, true)?;
    check_len("the summary", &form.summary, MAX_TEXT_CHARS, false)?;
    if let Some(git_ref) = &form.git_ref {
        check_git_ref(git_ref)?;
    }
    if form.problems.is_empty() {
        return Err(ApiError::invalid("a fix answers at least one problem"));
    }
    let largest = [
        form.problems.len(),
        form.tactics.len(),
        form.files.len(),
        form.comparisons.len(),
        form.steps.len(),
        form.baseline.len(),
        form.proof.len(),
    ]
    .into_iter()
    .max()
    .unwrap_or_default();
    if largest > MAX_ITEMS {
        return Err(ApiError::invalid(format!(
            "a fix takes at most {MAX_ITEMS} problems, tactics, files, comparisons, steps and \
             selections of each role"
        )));
    }
    form.comparisons.iter().try_for_each(check_comparison)?;
    form.steps.iter().try_for_each(check_step)
}

/// Validates a review and returns the problem status transition it triggers, if any.
pub fn validate_review(form: &NewReview) -> Result<Option<ProblemStatus>, ApiError> {
    check_len("the comment", &form.comment, MAX_TEXT_CHARS, false)?;
    match form.outcome {
        FixStatus::PendingReview => Err(ApiError::invalid(
            "a review approves, requests changes or supersedes",
        )),
        FixStatus::Approved => Ok(Some(ProblemStatus::Resolved)),
        FixStatus::ChangesRequested => Ok(Some(ProblemStatus::InProgress)),
        FixStatus::Superseded => Ok(None),
    }
}
