//! Design records: the briefs, directions, tactics and decisions behind a project's interface.

use serde::{Deserialize, Serialize};

text_enum! {
    /// What a design record holds.
    DesignKind {
        Brief => "brief",
        Direction => "direction",
        Tactic => "tactic",
        Decision => "decision",
    }
}

text_enum! {
    /// Where a design record stands.
    DesignStatus {
        Proposed => "proposed",
        Adopted => "adopted",
        Rejected => "rejected",
    }
}

/// `POST /v1/projects/{id}/design`
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct NewDesignEntry {
    pub kind: DesignKind,
    pub status: DesignStatus,
    pub title: String,
    #[serde(default)]
    pub note: String,
    /// The part of the interface the record is about.
    #[serde(default)]
    pub surface: Option<String>,
    /// Id of the record of this project this one replaces.
    #[serde(default)]
    pub supersedes: Option<String>,
    /// A document or link the record rests on.
    #[serde(default)]
    pub reference: Option<String>,
    /// Artifact id of a capture file that backs the record.
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DesignEntry {
    pub id: String,
    #[serde(flatten)]
    pub entry: NewDesignEntry,
    pub author: Option<String>,
    pub created_at: i64,
}
