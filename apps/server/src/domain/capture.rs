//! Capture review note validation.

use crate::domain::ApiError;

/// Longest review note, in characters.
const MAX_NOTE_CHARS: usize = 4000;

/// A review note, trimmed, once it has 1 to [`MAX_NOTE_CHARS`] characters.
pub fn clean_note(note: &str) -> Result<&str, ApiError> {
    let note = note.trim();
    if (1..=MAX_NOTE_CHARS).contains(&note.chars().count()) {
        Ok(note)
    } else {
        Err(ApiError::invalid(format!(
            "the note must have 1–{MAX_NOTE_CHARS} characters"
        )))
    }
}
