//! What a machine token, device or invitation must look like.

use super::{ApiError, Principal};
use rupicola_api::TokenKind;
use std::ops::RangeInclusive;

pub const HOUR_MS: i64 = 60 * 60 * 1000;
/// Longest label of a machine token or device.
const MAX_LABEL_CHARS: usize = 100;
/// Validity of an invitation when the owner does not choose one.
pub const DEFAULT_INVITE_HOURS: u32 = 24;
/// Shortest and longest validity of an invitation (up to one week).
const INVITE_HOURS: RangeInclusive<i64> = 1..=168;

pub fn clean_label(label: &str) -> Result<String, ApiError> {
    let label = label.trim();
    if (1..=MAX_LABEL_CHARS).contains(&label.chars().count()) {
        Ok(label.to_owned())
    } else {
        Err(ApiError::invalid(format!(
            "the label must have 1–{MAX_LABEL_CHARS} characters"
        )))
    }
}

/// Fails unless an invitation may last `hours`.
pub fn check_invite_hours(hours: i64) -> Result<(), ApiError> {
    if INVITE_HOURS.contains(&hours) {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "an invitation lasts {}–{} hours",
            INVITE_HOURS.start(),
            INVITE_HOURS.end()
        )))
    }
}

impl Principal {
    /// Only a signed-in person may manage credentials: a machine token or device
    /// cannot mint or list others.
    pub fn session_person(&self) -> Result<&str, ApiError> {
        if self.kind == TokenKind::Session {
            self.person()
        } else {
            Err(ApiError::Forbidden)
        }
    }
}
