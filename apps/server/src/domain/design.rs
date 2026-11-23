//! What a design record must look like.

use super::ApiError;
use rupicola_api::NewDesignEntry;

/// Longest title, in characters.
const MAX_TITLE_CHARS: usize = 200;
/// Longest note, in characters.
const MAX_NOTE_CHARS: usize = 8000;
/// Longest surface or reference, in characters.
const MAX_LABEL_CHARS: usize = 512;

pub fn validate(form: &NewDesignEntry) -> Result<(), ApiError> {
    let title = form.title.trim().chars().count();
    if !(1..=MAX_TITLE_CHARS).contains(&title) {
        return Err(ApiError::invalid(format!(
            "the title must have 1–{MAX_TITLE_CHARS} characters"
        )));
    }
    if form.note.chars().count() > MAX_NOTE_CHARS {
        return Err(ApiError::invalid(format!(
            "the note must have at most {MAX_NOTE_CHARS} characters"
        )));
    }
    for value in [&form.surface, &form.reference].into_iter().flatten() {
        if value.chars().count() > MAX_LABEL_CHARS {
            return Err(ApiError::invalid(format!(
                "the surface and the reference must have at most {MAX_LABEL_CHARS} characters"
            )));
        }
    }
    Ok(())
}
