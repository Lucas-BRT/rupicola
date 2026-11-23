use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum FixStatus {
    PendingReview,
    Approved,
    ChangesRequested,
    Superseded,
}

impl FixStatus {
    pub const ALL: &'static [Self] = &[
        Self::PendingReview,
        Self::Approved,
        Self::ChangesRequested,
        Self::Superseded,
    ];

    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::PendingReview => "pending-review",
            Self::Approved => "approved",
            Self::ChangesRequested => "changes-requested",
            Self::Superseded => "superseded",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::PendingReview => "Pending review",
            Self::Approved => "Approved",
            Self::ChangesRequested => "Changes requested",
            Self::Superseded => "Superseded",
        }
    }

    #[must_use]
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "pending-review" => Some(Self::PendingReview),
            "approved" => Some(Self::Approved),
            "changes-requested" => Some(Self::ChangesRequested),
            "superseded" => Some(Self::Superseded),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ComparisonPair {
    pub label: String,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub before_url: String,
    pub after_url: String,
    #[serde(default)]
    pub diff_url: Option<String>,
    #[serde(default)]
    pub changed_ratio: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixStep {
    pub index: usize,
    pub title: String,
    pub detail: String,
    pub verification: String,
    #[serde(default)]
    pub evidence_url: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixReview {
    pub author: String,
    pub action: String,
    pub comment: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixRecord {
    pub id: String,
    pub project_id: String,
    #[serde(default)]
    pub problem_ids: Vec<String>,
    pub title: String,
    pub summary: String,
    pub status: FixStatus,
    #[serde(default)]
    pub tactics: Vec<String>,
    #[serde(default)]
    pub modified_files: Vec<String>,
    #[serde(default)]
    pub comparisons: Vec<ComparisonPair>,
    #[serde(default)]
    pub steps: Vec<FixStep>,
    #[serde(default)]
    pub reviews: Vec<FixReview>,
    pub author: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}
