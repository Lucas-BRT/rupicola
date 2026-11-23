//! Capture removal rules: cited-deletion policy, confirmation tokens and reason validation.

use crate::domain::{Access, ApiError};
use rupicola_api::{Citation, DeletionPolicy, Role};
use sha2::{Digest, Sha256};

/// The project setting that holds the policy.
pub const POLICY_KEY: &str = "cited_capture_deletion";
/// The policy of a project that never chose one.
pub const DEFAULT_POLICY: DeletionPolicy = DeletionPolicy::Confirm;
/// Longest reason a removal may give, in characters.
const MAX_REASON_CHARS: usize = 500;

/// Whether the server lets any project remove a cited capture, from `RUPICOLA_CITED_DELETION`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum CitedDeletion {
    /// Each project's policy decides.
    #[default]
    Allow,
    /// No cited capture is ever removed.
    Never,
}

/// Trims and checks the removal reason length.
///
/// # Errors
///
/// [`ApiError::Invalid`] when the reason is empty or exceeds [`MAX_REASON_CHARS`].
pub fn check_reason(reason: &str) -> Result<&str, ApiError> {
    let reason = reason.trim();
    if !(1..=MAX_REASON_CHARS).contains(&reason.chars().count()) {
        return Err(ApiError::invalid(format!(
            "say why the capture goes, in at most {MAX_REASON_CHARS} characters"
        )));
    }
    Ok(reason)
}

/// What a removal must send back: a digest of the capture and of what cites it, so a citation
/// added after the caller looked makes the confirmation stale.
#[must_use]
pub fn confirmation_token(capture: &str, citations: &[Citation]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(capture.as_bytes());
    for citation in citations {
        hasher.update(b"\n");
        hasher.update(citation.kind.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(citation.id.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Whether the caller may remove a capture with these citations under `policy` and `server_allows`.
#[must_use]
pub fn is_allowed(
    server_allows: bool,
    seen: Access,
    citations: &[Citation],
    policy: DeletionPolicy,
) -> bool {
    match seen {
        Access::Device => false,
        Access::Person(role) if citations.is_empty() => role >= Role::Reviewer,
        Access::Person(role) => {
            role >= Role::Owner && policy == DeletionPolicy::Confirm && server_allows
        }
    }
}
