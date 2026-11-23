//! What a project id and label must look like.

use super::ApiError;

/// Longest project id and label.
pub const MAX_ID_LEN: usize = 100;
const MAX_LABEL_CHARS: usize = 100;

/// Project ids are client-chosen slugs, so they double as safe path segments.
pub fn valid_id(id: &str) -> bool {
    (1..=MAX_ID_LEN).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Fails unless `id` is a valid project id.
pub fn check_id(id: &str) -> Result<(), ApiError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "the project id must have 1–{MAX_ID_LEN} letters, digits, - or _"
        )))
    }
}

pub fn clean_label(label: &str) -> Result<String, ApiError> {
    let label = label.trim();
    if (1..=MAX_LABEL_CHARS).contains(&label.chars().count()) {
        Ok(label.to_owned())
    } else {
        Err(ApiError::invalid(format!(
            "the project label must have 1–{MAX_LABEL_CHARS} characters"
        )))
    }
}
