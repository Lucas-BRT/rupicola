//! Unified domain layer shared across CLI, Panel HTTP, and MCP entrypoints.
//! Connects Phase 1 (`rupicola-capture` / `rupicola-protocol`) to Phase 2 queues.

use rupicola_protocol::UiSnapshot;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Files one capture produced, as the panel index stores them.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CaptureOutput {
    pub screenshot_png: PathBuf,
    #[serde(default)]
    pub annotated_png: Option<PathBuf>,
    #[serde(default)]
    pub video_mp4: Option<PathBuf>,
    #[serde(default)]
    pub annotated_video_mp4: Option<PathBuf>,
    #[serde(default)]
    pub video_gif: Option<PathBuf>,
    #[serde(default)]
    pub annotated_video_gif: Option<PathBuf>,
    #[serde(default)]
    pub snapshot: Option<UiSnapshot>,
}

/// Lifecycle state of a capture inside `CaptureQueue`.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureQueueStatus {
    #[default]
    PendingAnalysis,
    Analyzed,
    Archived,
}

impl CaptureQueueStatus {
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::PendingAnalysis => "pending-analysis",
            Self::Analyzed => "analyzed",
            Self::Archived => "archived",
        }
    }
}

/// Persisted capture record bridging [`CaptureOutput`] into the panel index.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CaptureRecord {
    pub id: String,
    pub project_id: String,
    pub backend: String,
    pub target: String,
    pub status: CaptureQueueStatus,
    pub output: CaptureOutput,
    pub created_at_ms: u64,
}

/// Explicit human or automated triage decision on a problem in `ProblemsQueue`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "kebab-case")]
pub enum ProblemDecision {
    ApproveForFix,
    RequestNewEvidence { require_video: bool, note: String },
    MarkFalsePositive { note: String },
    Delete,
}

/// Explicit review decision on a proposed or implemented solution in `SolutionsQueue`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "kebab-case")]
pub enum SolutionDecision {
    RequestChanges { note: String },
    ApproveVisual,
    ImplementInRepo,
}
