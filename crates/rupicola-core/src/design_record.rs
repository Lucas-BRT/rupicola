use serde::{Deserialize, Serialize};

pub const DIRECTORY: &str = "design";

#[derive(Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Brief,
    Direction,
    Tactic,
    Decision,
}

impl Kind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Brief => "Brief",
            Self::Direction => "Art direction",
            Self::Tactic => "Tactics",
            Self::Decision => "Decisions",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Proposed,
    Adopted,
    Rejected,
}

impl Status {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Proposed => "Proposed",
            Self::Adopted => "Adopted",
            Self::Rejected => "Rejected",
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Entry {
    pub id: String,
    pub timestamp_ms: u64,
    pub kind: Kind,
    pub status: Status,
    pub title: String,
    pub note: String,
    pub surface: Option<String>,
    pub supersedes: Option<String>,
    pub reference: Option<String>,
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

#[must_use]
pub fn superseded(entries: &[Entry]) -> std::collections::BTreeSet<&str> {
    entries
        .iter()
        .filter_map(|entry| entry.supersedes.as_deref())
        .collect()
}
