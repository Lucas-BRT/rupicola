//! Unified domain layer shared across CLI, Panel HTTP, and MCP entrypoints.
//! Connects Phase 1 (`rupicola-capture` / `rupicola-protocol`) to Phase 2 queues.

pub use rupicola_capture::{CaptureOutput, CaptureRequest};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

/// Persisted capture record bridging `rupicola_capture::CaptureOutput` into the panel index.
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

/// Builds a targeted Phase 1 [`CaptureRequest`] when a reviewer requests new evidence
/// (for example upgrading a static screenshot to a video walkthrough).
#[must_use]
pub fn build_reprocess_request(
    base: &CaptureRequest,
    require_video: bool,
    output_dir: PathBuf,
) -> CaptureRequest {
    let mut next = base.clone();
    next.output_dir = output_dir;
    if require_video {
        next.record_video_ms = Some(next.record_video_ms.unwrap_or(2000).max(1500));
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reprocess_request_enables_video_when_required() {
        let base = CaptureRequest {
            command: "http://127.0.0.1:4732".into(),
            args: vec![],
            viewport: [1280, 720],
            warmup_ms: 0,
            actions: vec![],
            record_video_ms: None,
            annotations: vec![],
            output_dir: PathBuf::from("/tmp/old"),
        };
        let next = build_reprocess_request(&base, true, PathBuf::from("/tmp/new"));
        assert_eq!(next.output_dir, PathBuf::from("/tmp/new"));
        assert_eq!(next.record_video_ms, Some(2000));
    }
}
