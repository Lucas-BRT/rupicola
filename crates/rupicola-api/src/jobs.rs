//! Capture jobs: a request to run a scenario on trees of the code, taken by a runner.

use serde::{Deserialize, Serialize};

text_enum! {
    /// Where a job stands.
    JobStatus {
        /// Waiting for a runner.
        Queued => "queued",
        /// A runner has it and reports how many captures it has made.
        Running => "running",
        /// Every capture was made and uploaded.
        Done => "done",
        /// The runner gave up; `error` says why.
        Failed => "failed",
        /// Someone cancelled it before it finished.
        Cancelled => "cancelled",
    }
}

impl JobStatus {
    /// Whether a runner still has something to do: the job is waiting or being worked on.
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

/// A tree of the code to capture: a git ref, with other refs merged on top.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct TreeSpec {
    /// Names the tree inside the job, such as `base` or `base+fix`.
    pub name: String,
    /// Git ref the runner checks out.
    pub base: String,
    /// Refs the runner merges on top, in order.
    #[serde(default)]
    pub merge: Vec<String>,
    /// Fixes those merges stand for, kept so a comparison knows what its trees had applied.
    #[serde(default)]
    pub fixes: Vec<String>,
}

/// `POST /v1/projects/{id}/jobs`
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct NewJob {
    /// Name of a scenario the project has.
    pub scenario: String,
    /// Exact version to run; the latest when absent.
    #[serde(default)]
    pub hash: Option<String>,
    /// The trees to capture, one set of captures each.
    pub trees: Vec<TreeSpec>,
    /// The fix this job's captures are for, when there is one.
    #[serde(default)]
    pub fix: Option<String>,
}

/// A capture a runner made for a job: which tree, viewport and theme it is.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct JobCapture {
    /// A capture of the same project, already uploaded.
    pub capture: String,
    pub tree: String,
    pub viewport: String,
    #[serde(default)]
    pub theme: String,
}

/// A capture job.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Job {
    pub id: String,
    pub project_id: String,
    pub scenario: String,
    /// Version of the scenario the job runs.
    pub hash: String,
    pub fix: Option<String>,
    pub trees: Vec<TreeSpec>,
    pub status: JobStatus,
    /// Captures the whole job makes: the scenario's matrix times the trees.
    pub cells_total: u32,
    pub cells_done: u32,
    /// Who has the job.
    pub runner: Option<String>,
    pub error: Option<String>,
    pub captures: Vec<JobCapture>,
    pub created_by: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

/// `POST /v1/jobs/{id}/progress`: how far the runner is. Also tells the server it is alive.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct JobProgress {
    pub cells_done: u32,
}

/// `POST /v1/jobs/{id}/complete`
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct JobResult {
    pub captures: Vec<JobCapture>,
}

/// `POST /v1/jobs/{id}/fail`
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct JobFailure {
    pub error: String,
}

/// Default base Git ref used when composing comparison trees.
pub const DEFAULT_BASE_REF: &str = "develop";
/// Default automatic recapture setting after a fix is approved.
pub const DEFAULT_AUTO_RECAPTURE: bool = true;

/// Project or server capture settings controlling automatic recapture and base branch.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RecaptureSettings {
    pub auto_recapture: bool,
    pub base_ref: String,
}

/// `PATCH /v1/projects/{id}/capture-settings` or `PATCH /v1/settings/capture`
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct RecaptureSettingsPatch {
    #[serde(default)]
    pub auto_recapture: Option<bool>,
    #[serde(default)]
    pub base_ref: Option<String>,
}
