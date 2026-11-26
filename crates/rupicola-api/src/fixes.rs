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

impl FixStatus {
    /// Human-readable label shown in the UI.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PendingReview => "Pending review",
            Self::Approved => "Approved",
            Self::ChangesRequested => "Changes requested",
            Self::Superseded => "Superseded",
        }
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

text_enum! {
    /// Which tree a comparison was captured on.
    #[derive(Default)]
    ComparisonView {
        /// The base against the base plus this fix alone: what the fix does by itself.
        #[default]
        Isolated => "isolated",
        /// The base with every approved fix against the same plus this fix: how the fix looks
        /// once the approved ones are in.
        Integrated => "integrated",
    }
}

/// Before/after proof for one route, viewport and theme. The artifacts are files of
/// captures in the same project.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Comparison {
    pub label: String,
    #[serde(default)]
    pub view: ComparisonView,
    /// Source revision the before capture was taken at.
    #[serde(default)]
    pub base_revision: String,
    /// Source revision the after capture was taken at.
    #[serde(default)]
    pub revision: String,
    /// Fixes the before tree already had applied; empty for an isolated comparison.
    #[serde(default)]
    pub applied_fixes: Vec<String>,
    /// An integrated comparison is stale once the approved fixes are not the ones it was taken
    /// with. Set when a fix is read; whatever a client sends is ignored.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
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

impl Comparison {
    /// Whether this comparison no longer matches the project: an integrated one whose
    /// `applied_fixes` differ from `approved`, the approved fixes other than `own`. An
    /// isolated comparison does not depend on other fixes.
    #[must_use]
    pub fn is_stale(&self, own: &str, approved: &std::collections::BTreeSet<String>) -> bool {
        if self.view != ComparisonView::Integrated {
            return false;
        }
        let applied: std::collections::BTreeSet<&str> =
            self.applied_fixes.iter().map(String::as_str).collect();
        let current: std::collections::BTreeSet<&str> = approved
            .iter()
            .map(String::as_str)
            .filter(|id| *id != own)
            .collect();
        applied != current
    }
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
    /// Git ref (branch or commit) holding this fix's changes, for isolated/integrated runner jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
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
    /// Explicit reason when before/after captures intentionally differ in `plan_spec` (human-only by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mismatch_override_reason: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    pub status: FixStatus,
    pub problems: Vec<String>,
    pub tactics: Vec<String>,
    pub files: Vec<String>,
    pub comparisons: Vec<Comparison>,
    /// The selections the fix cites: the comparisons' before and after files, and any other.
    pub selections: Vec<FixSelection>,
    pub steps: Vec<Step>,
    pub reviews: Vec<Review>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mismatch_override_reason: Option<String>,
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
