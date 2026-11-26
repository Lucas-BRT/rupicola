//! Composable workspace, media-analysis, runner, and admin operation traits.
//!
//! Every atomic capability trait inherits [`OpsBase`] so supertraits compose cleanly
//! without associated-type conflicts. Supertraits use blanket implementations: any
//! client that implements the required atomic traits automatically satisfies the
//! corresponding supertrait.

mod admin;
mod domain;
mod dto;
mod supertraits;

pub use admin::*;
pub use domain::*;
pub use dto::*;
pub use supertraits::*;

/// Shared error contract inherited by every atomic operation trait.
pub trait OpsBase: Send + Sync {
    type Error: Send + Sync;
}

/// Canonical operation names matching the active [`WorkspaceOps`] supertrait 1:1.
/// The MCP server test asserts every name here is exposed in `TOOLS`.
pub const WORKSPACE_OP_NAMES: &[&str] = &[
    // Identity & projects
    "me",
    "project_list",
    "project_get",
    "project_create",
    "project_update",
    "project_delete",
    "project_limits",
    "capture_settings_get",
    "capture_settings_update",
    // Captures & artifacts
    "capture_list",
    "capture_search",
    "capture_get",
    "capture_capabilities",
    "capture_publish",
    "capture_review",
    "capture_citations",
    "capture_delete",
    "capture_lineage",
    "capture_parents_get",
    "capture_parents_edit",
    "capture_compare",
    "artifact_read",
    // Problems
    "problem_list",
    "problem_list_detailed",
    "problem_get",
    "problem_create",
    "problem_delete",
    "problem_triage",
    "problem_comment",
    "problem_analyze",
    "problem_cite_selection",
    "problem_uncite_selection",
    // Fixes
    "fix_list",
    "fix_list_detailed",
    "fix_get",
    "fix_create",
    "fix_step",
    "fix_review",
    "fix_cite_selection",
    "fix_uncite_selection",
    // Design & design systems
    "design_list",
    "design_record",
    "design_system_list",
    "design_system_get",
    "design_system_create",
    "design_system_edit",
    "design_system_act",
    // Jobs (including runner lifecycle)
    "job_list",
    "job_get",
    "job_create",
    "job_claim",
    "job_progress",
    "job_complete",
    "job_fail",
    "job_cancel",
    // Scenarios
    "scenario_list",
    "scenario_get",
    "scenario_put",
    // Selections
    "selection_create",
    "selection_get",
    // Topics
    "topic_tree",
    "topic_create",
    "topic_rename",
    "topic_move",
    "topic_delete",
    "topic_captures",
    "topic_file_capture",
    "topic_unfile_capture",
    // Offline static analysis (rupicola-core)
    "offline_discover",
    "offline_scan",
    "offline_check",
    "offline_rules",
    "offline_modules",
    // Capture execution (via rupicola-capture)
    "capture_run_scenario",
    "capture_record_clip",
    "capture_snapshot_checkpoint",
    // Progressive token-efficient video/media inspection (standardized ffmpeg strategy)
    "media_probe",
    "media_motion_diff",
    "media_contact_sheet",
    "media_sample_frames",
    // Visual diff & targeted crop
    "visual_diff_images",
    "visual_crop_region",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn workspace_op_names_are_unique() {
        let unique: BTreeSet<&str> = WORKSPACE_OP_NAMES.iter().copied().collect();
        assert_eq!(unique.len(), WORKSPACE_OP_NAMES.len());
    }

    #[test]
    fn progressive_frame_sample_strategies_round_trip_json() {
        let strategies = [
            FrameSampleStrategy::Coarse { count: 15 },
            FrameSampleStrategy::Bisect {
                start_ms: 200,
                end_ms: 800,
                count: 15,
                exclude_frames: vec![0, 8, 16],
            },
            FrameSampleStrategy::Window {
                start_ms: 350,
                end_ms: 450,
                fps: 60,
            },
        ];
        for s in strategies {
            let json = serde_json::to_string(&s).unwrap();
            let back: FrameSampleStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn visual_diff_and_media_dtos_round_trip_json() {
        let probe = MediaProbe {
            artifact_id: "art-1".to_owned(),
            mime: "video/mp4".to_owned(),
            width: 1280,
            height: 720,
            duration_ms: Some(1500),
            fps: Some(60.0),
            frame_count: Some(90),
            codec: Some("h264".to_owned()),
        };
        let back: MediaProbe =
            serde_json::from_str(&serde_json::to_string(&probe).unwrap()).unwrap();
        assert_eq!(back, probe);

        let diff = VisualDiffReport {
            before_artifact_id: "a".to_owned(),
            after_artifact_id: "b".to_owned(),
            diff_artifact_id: "d".to_owned(),
            changed_ratio: 0.125,
            changed_regions: vec![CropRect {
                x: 32,
                y: 64,
                width: 96,
                height: 32,
            }],
        };
        let back: VisualDiffReport =
            serde_json::from_str(&serde_json::to_string(&diff).unwrap()).unwrap();
        assert_eq!(back, diff);
    }
}
