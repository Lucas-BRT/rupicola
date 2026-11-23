//! Problems: defects found in a project's interface.

use serde::{Deserialize, Serialize};

text_enum! {
    /// Where a problem stands in triage.
    ProblemStatus {
        Open => "open",
        InProgress => "in-progress",
        Intentional => "intentional",
        FalsePositive => "false-positive",
        NeedsReprocess => "needs-reprocess",
        Resolved => "resolved",
    }
}

text_enum! {
    Severity {
        Critical => "critical",
        Major => "major",
        Minor => "minor",
        Polish => "polish",
    }
}

text_enum! {
    /// How a problem was found.
    ProblemOrigin {
        AgentMeasured => "agent-measured",
        AgentVisual => "agent-visual",
        UserReported => "user-reported",
    }
}

/// A design rule a problem violates.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Rule {
    pub scope: String,
    pub code: String,
    pub title: String,
}

/// `POST /v1/projects/{id}/problems`. `evidence` lists artifact ids of the same project.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct NewProblem {
    pub title: String,
    pub severity: Severity,
    pub origin: ProblemOrigin,
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub viewport: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub diagnosis: String,
    #[serde(default)]
    pub proposed_solution: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Files that show the problem, by artifact id; they become one selection of proof.
    #[serde(default)]
    pub evidence: Vec<String>,
    /// Selections that show the problem, made with it.
    #[serde(default)]
    pub selections: Vec<crate::NewSelection>,
}

/// `PATCH /v1/problems/{id}`: triage. Absent fields stay as they are.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ProblemPatch {
    #[serde(default)]
    pub status: Option<ProblemStatus>,
    #[serde(default)]
    pub severity: Option<Severity>,
    #[serde(default)]
    pub proposed_solution: Option<String>,
}

/// `POST /v1/problems/{id}/comments`
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct NewComment {
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Comment {
    pub id: String,
    pub author: Option<String>,
    pub text: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: ProblemStatus,
    pub severity: Severity,
    pub origin: ProblemOrigin,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub diagnosis: String,
    pub proposed_solution: String,
    pub rules: Vec<Rule>,
    /// The artifact ids every selection holds up as proof, each once.
    pub evidence: Vec<String>,
    /// The selections the problem cites.
    pub selections: Vec<crate::Selection>,
    pub comments: Vec<Comment>,
    pub author: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One row of `GET /v1/projects/{id}/problems`, most recently updated first.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ProblemSummary {
    pub id: String,
    pub title: String,
    pub status: ProblemStatus,
    pub severity: Severity,
    pub origin: ProblemOrigin,
    pub route: String,
    pub updated_at: i64,
}
