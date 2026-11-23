//! Selection domain rules: validating a selection's label and anchor count.

use crate::domain::ApiError;
use rupicola_api::{MAX_SELECTION_ITEMS, MAX_SELECTION_LABEL_CHARS, NewSelection};

/// Checks a new selection's label and anchor count before it is stored.
///
/// # Errors
///
/// [`ApiError::Invalid`] when the label is empty or too long, or the anchor count is out of range.
pub fn validate(form: &NewSelection) -> Result<&str, ApiError> {
    let label = form.label.trim();
    if !(1..=MAX_SELECTION_LABEL_CHARS).contains(&label.chars().count()) {
        return Err(ApiError::invalid(format!(
            "a selection needs a label of at most {MAX_SELECTION_LABEL_CHARS} characters"
        )));
    }
    if !(1..=MAX_SELECTION_ITEMS).contains(&form.items.len()) {
        return Err(ApiError::invalid(format!(
            "a selection holds 1 to {MAX_SELECTION_ITEMS} anchors"
        )));
    }
    Ok(label)
}
