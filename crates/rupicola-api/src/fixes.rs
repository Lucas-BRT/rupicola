//! Fixes: changes that answer problems, with before/after proof and review.

use serde::{Deserialize, Serialize};

text_enum! {
    /// Where a fix stands in review.
    FixStatus {
        PendingReview => "pending-review",
        Approved => "approved",
        ChangesRequested => "changes-requested",
        Superseded => "superseded",
    }
}

text_enum! {
    /// What a selection is to a fix.
    FixSelectionRole {
        /// How things were before the fix.
        Baseline => "baseline",
        /// That the fix works.
        SolutionProof => "solution-proof",
    }
}

/// A selection a fix cites, and what it is to the fix.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FixSelection {
    pub role: FixSelectionRole,
    pub selection: crate::Selection,
}

/// `PUT /v1/fixes/{id}/selections/{selection}`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FixSelectionLink {
    pub role: FixSelectionRole,
}

/// Before/after proof for one route, viewport and theme. The artifacts are files of
/// captures in the same project.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Comparison {
    pub label: String,
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub viewport: String,
    #[serde(default)]
    pub theme: String,
    pub before: String,
    pub after: String,
    #[serde(default)]
    pub diff: Option<String>,
    #[serde(default)]
    pub changed_ratio: Option<f64>,
}

/// One verified step of the work, optionally backed by an artifact.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct NewStep {
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub verification: String,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Step {
    #[serde(flatten)]
    pub step: NewStep,
    pub created_at: i64,
}

/// `POST /v1/projects/{id}/fixes`
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct NewFix {
    pub title: String,
    #[serde(default)]
    pub summary: String,
    /// Problems this fix answers; at least one, all of the same project.
    pub problems: Vec<String>,
    #[serde(default)]
    pub tactics: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub comparisons: Vec<Comparison>,
    #[serde(default)]
    pub steps: Vec<NewStep>,
    /// Selections of how things were, made with the fix.
    #[serde(default)]
    pub baseline: Vec<crate::NewSelection>,
    /// Selections that show the fix works, made with it.
    #[serde(default)]
    pub proof: Vec<crate::NewSelection>,
}

/// `POST /v1/fixes/{id}/reviews`: the outcome moves the fix and, for approval or
/// requested changes, its problems.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct NewReview {
    pub outcome: FixStatus,
    #[serde(default)]
    pub comment: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Review {
    pub author: Option<String>,
    pub outcome: FixStatus,
    pub comment: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Fix {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub summary: String,
    pub status: FixStatus,
    pub problems: Vec<String>,
    pub tactics: Vec<String>,
    pub files: Vec<String>,
    pub comparisons: Vec<Comparison>,
    /// The selections the fix cites: the comparisons' before and after files, and any other.
    pub selections: Vec<FixSelection>,
    pub steps: Vec<Step>,
    pub reviews: Vec<Review>,
    pub author: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One row of `GET /v1/projects/{id}/fixes`, most recently updated first.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FixSummary {
    pub id: String,
    pub title: String,
    pub status: FixStatus,
    pub problems: usize,
    pub updated_at: i64,
}
