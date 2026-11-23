//! Removing a capture: who cites it, whether the project lets a cited capture go, and the
//! tombstone it leaves so what cited it still resolves.

use serde::{Deserialize, Serialize};

text_enum! {
    /// Whether a project lets a capture that something cites be removed.
    DeletionPolicy {
        /// Never.
        Forbid => "forbid",
        /// With an explicit confirmation of what cites it.
        Confirm => "confirm",
    }
}

text_enum! {
    /// What cites a capture.
    CitationKind {
        Problem => "problem",
        Fix => "fix",
        Design => "design",
        /// A selection that nothing cites yet.
        Selection => "selection",
        /// A review note on another capture.
        Finding => "finding",
    }
}

/// One thing that cites the capture.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Citation {
    pub kind: CitationKind,
    pub id: String,
    pub title: String,
    pub status: Option<String>,
}

/// `GET /v1/captures/{id}/citations`: what removing the capture would touch.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Citations {
    pub citations: Vec<Citation>,
    /// Captures whose lineage starts from this one; they stay, and do not block the removal.
    pub children: Vec<String>,
    pub policy: DeletionPolicy,
    /// Whether the caller may remove it at all.
    pub allowed: bool,
    /// What a removal must send back to confirm it saw these citations; none when nothing
    /// cites the capture.
    pub confirm: Option<String>,
}

/// `DELETE /v1/captures/{id}`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CaptureRemoval {
    pub reason: String,
    #[serde(default)]
    pub confirm: Option<String>,
}

/// What is left of a removed capture.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Tombstone {
    pub at: i64,
    pub by: String,
    pub reason: String,
}

/// `GET` and `PUT /v1/projects/{id}/deletion-policy`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PolicyView {
    pub policy: DeletionPolicy,
    /// Whether the server lets any project remove a cited capture.
    #[serde(default = "server_allows_by_default")]
    pub server_allows: bool,
}

const fn server_allows_by_default() -> bool {
    true
}
