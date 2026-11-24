//! Problem domain rules: field bounds for filing, triaging and commenting.

use crate::domain::ApiError;
use rupicola_api::{NewProblem, ProblemPatch};

/// Longest problem title, in characters.
const MAX_TITLE_CHARS: usize = 200;
/// Longest diagnosis, proposed solution or comment, in characters.
const MAX_TEXT_CHARS: usize = 8000;
/// Longest route, viewport or theme label, in characters.
const MAX_LABEL_CHARS: usize = 256;
/// Rules one problem may cite.
const MAX_RULES: usize = 32;
/// Evidence artifacts one problem may cite.
const MAX_EVIDENCE: usize = 64;
/// Selections a new problem may bring.
const MAX_SELECTIONS: usize = 16;
/// The label of the selection a problem's evidence files become.
pub const EVIDENCE_LABEL: &str = "Evidence";

/// Bounds on text fields, as an error message when one is out of range.
pub fn check_len(field: &str, value: &str, max: usize, required: bool) -> Result<(), ApiError> {
    let min = usize::from(required);
    if (min..=max).contains(&value.chars().count()) {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "{field} must have {min}–{max} characters"
        )))
    }
}

/// Field bounds of a new problem.
///
/// # Errors
///
/// [`ApiError::Invalid`] when any field or collection exceeds its limit.
pub fn validate(form: &NewProblem) -> Result<(), ApiError> {
    check_len("the title", form.title.trim(), MAX_TITLE_CHARS, true)?;
    check_len("the diagnosis", &form.diagnosis, MAX_TEXT_CHARS, false)?;
    check_len(
        "the proposed solution",
        &form.proposed_solution,
        MAX_TEXT_CHARS,
        false,
    )?;
    for (field, value) in [
        ("the route", &form.route),
        ("the viewport", &form.viewport),
        ("the theme", &form.theme),
    ] {
        check_len(field, value, MAX_LABEL_CHARS, false)?;
    }
    if form.rules.len() > MAX_RULES
        || form.evidence.len() > MAX_EVIDENCE
        || form.selections.len() > MAX_SELECTIONS
    {
        return Err(ApiError::invalid(format!(
            "a problem takes at most {MAX_RULES} rules, {MAX_EVIDENCE} evidence files and \
             {MAX_SELECTIONS} selections"
        )));
    }
    Ok(())
}

/// Field bounds of a problem patch.
pub fn validate_patch(patch: &ProblemPatch) -> Result<(), ApiError> {
    if let Some(solution) = &patch.proposed_solution {
        check_len("the proposed solution", solution, MAX_TEXT_CHARS, false)?;
    }
    Ok(())
}

/// Trims and validates a problem comment.
pub fn validate_comment(text: &str) -> Result<&str, ApiError> {
    let text = text.trim();
    check_len("the comment", text, MAX_TEXT_CHARS, true)?;
    Ok(text)
}
