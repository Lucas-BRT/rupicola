//! What a username and a password must look like, and how long a sign-in lasts.

use super::ApiError;
use std::ops::RangeInclusive;

/// How long a sign-in stays valid, in milliseconds (7 days).
pub const SESSION_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Shortest and longest accepted username.
const HANDLE_LEN: RangeInclusive<usize> = 3..=48;
/// Shortest and longest accepted password, in bytes.
pub const PASSWORD_LEN: RangeInclusive<usize> = 12..=1024;

/// The 422 sent for a username or password outside the accepted rules.
pub fn account_rules() -> ApiError {
    ApiError::invalid(format!(
        "use a {}–{} character username (letters, digits, - or _) and a password of {}–{} bytes",
        HANDLE_LEN.start(),
        HANDLE_LEN.end(),
        PASSWORD_LEN.start(),
        PASSWORD_LEN.end()
    ))
}

pub fn valid_handle(handle: &str) -> bool {
    HANDLE_LEN.contains(&handle.len())
        && handle
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// A username as it is stored: trimmed and lowercase.
pub fn normal_handle(handle: &str) -> String {
    handle.trim().to_ascii_lowercase()
}
