//! Fine-grained atomic domain traits. Every trait inherits [`OpsBase`] so they
//! compose cleanly into scoped supertraits.

use super::{
    ArtifactChunk, CaptureRunRequest, ContactSheetRequest, CropRect, FrameSampleStrategy,
    MediaProbe, MotionDiffReport, OpsBase, PublishCaptureRequest, RecordClipRequest, SampledFrames,
    SnapshotCheckpointRequest, VisualDiffMode, VisualDiffReport,
};
use crate::{
    Capture, CaptureCapabilities, CaptureDiff, CapturePage, CaptureRemoval, CaptureSummary,
    Citations, Comment, DesignEntry, DocumentEdit, Finding, Fix, FixSelectionLink, FixSummary, Job,
    JobFailure, JobProgress, JobResult, Limits, Lineage, LineageGraph, Me, NewComment,
    NewDesignEntry, NewFinding, NewFix, NewJob, NewProblem, NewProject, NewReview, NewSelection,
    NewStep, NewTopic, ParentsEdit, Problem, ProblemPatch, ProblemSummary, Project, ProjectUpdate,
    RecaptureSettings, RecaptureSettingsPatch, Scenario, ScenarioSummary, ScenarioVersion,
    Selection, Step, SystemActionForm, SystemList, SystemVersion, Topic, TopicMove, TopicRename,
    TopicTree,
};
use async_trait::async_trait;
use serde_json::Value;

// ── Identity & projects ───────────────────────────────────────────────────────

#[async_trait]
pub trait IdentityOps: OpsBase {
    async fn me(&self) -> Result<Me, Self::Error>;
}

#[async_trait]
pub trait ProjectCrudOps: OpsBase {
    async fn project_list(&self) -> Result<Vec<Project>, Self::Error>;
    async fn project_get(&self, id: &str) -> Result<Project, Self::Error>;
    async fn project_create(&self, form: &NewProject) -> Result<Project, Self::Error>;
    async fn project_update(&self, id: &str, form: &ProjectUpdate) -> Result<Project, Self::Error>;
    async fn project_delete(&self, id: &str) -> Result<(), Self::Error>;
}

#[async_trait]
pub trait ProjectPolicyOps: OpsBase {
    async fn project_limits(&self, project: &str) -> Result<Limits, Self::Error>;
    async fn capture_settings_get(&self, project: &str) -> Result<RecaptureSettings, Self::Error>;
    async fn capture_settings_update(
        &self,
        project: &str,
        patch: &RecaptureSettingsPatch,
    ) -> Result<RecaptureSettings, Self::Error>;
}

// ── Captures & artifacts ──────────────────────────────────────────────────────

#[async_trait]
pub trait CaptureCatalogOps: OpsBase {
    async fn capture_list(&self, project: &str) -> Result<Vec<CaptureSummary>, Self::Error>;
    async fn capture_search(
        &self,
        project: &str,
        query: Option<&str>,
    ) -> Result<CapturePage, Self::Error>;
    async fn capture_get(&self, id: &str) -> Result<Capture, Self::Error>;
    async fn capture_capabilities(&self) -> Result<CaptureCapabilities, Self::Error>;
}

#[async_trait]
pub trait CapturePublishOps: OpsBase {
    async fn capture_publish(&self, req: &PublishCaptureRequest) -> Result<Capture, Self::Error>;
    async fn capture_review(&self, id: &str, finding: &NewFinding) -> Result<Finding, Self::Error>;
}

#[async_trait]
pub trait CaptureLifecycleOps: OpsBase {
    async fn capture_citations(&self, id: &str) -> Result<Citations, Self::Error>;
    async fn capture_delete(&self, id: &str, form: &CaptureRemoval) -> Result<(), Self::Error>;
    async fn capture_recapture(
        &self,
        id: &str,
        form: &crate::RecaptureRequest,
    ) -> Result<crate::Job, Self::Error>;
}

#[async_trait]
pub trait CaptureLineageOps: OpsBase {
    async fn capture_lineage(
        &self,
        id: &str,
        depth: Option<u32>,
    ) -> Result<LineageGraph, Self::Error>;
    async fn capture_parents_get(&self, id: &str) -> Result<Lineage, Self::Error>;
    async fn capture_parents_edit(
        &self,
        id: &str,
        form: &ParentsEdit,
    ) -> Result<Lineage, Self::Error>;
    async fn capture_compare(&self, before: &str, after: &str) -> Result<CaptureDiff, Self::Error>;
}

#[async_trait]
pub trait ArtifactReadOps: OpsBase {
    async fn artifact_read(
        &self,
        id: &str,
        offset: u64,
        length: usize,
    ) -> Result<ArtifactChunk, Self::Error>;
}

/// Capture-execution orchestration over `rupicola-capture` (no in-repo drivers).
#[async_trait]
pub trait CaptureExecutionOps: OpsBase {
    async fn capture_run_scenario(&self, req: &CaptureRunRequest) -> Result<Capture, Self::Error>;
    async fn capture_record_clip(&self, req: &RecordClipRequest) -> Result<Capture, Self::Error>;
    async fn capture_snapshot_checkpoint(
        &self,
        req: &SnapshotCheckpointRequest,
    ) -> Result<Capture, Self::Error>;
}

// ── Progressive token-efficient media & visual analysis ───────────────────────

/// Standardized, token-efficient video/media inspection (probe → motion diff →
/// contact sheet → progressive frame sampling without re-reading frames).
#[async_trait]
pub trait MediaInspectOps: OpsBase {
    async fn media_probe(&self, artifact_id: &str) -> Result<MediaProbe, Self::Error>;
    async fn media_motion_diff(
        &self,
        artifact_id: &str,
        threshold: f64,
    ) -> Result<MotionDiffReport, Self::Error>;
    async fn media_contact_sheet(
        &self,
        artifact_id: &str,
        req: &ContactSheetRequest,
    ) -> Result<ArtifactChunk, Self::Error>;
    async fn media_sample_frames(
        &self,
        artifact_id: &str,
        strategy: &FrameSampleStrategy,
    ) -> Result<SampledFrames, Self::Error>;
}

/// Standardized image diffing and region cropping to avoid full-screen token waste.
#[async_trait]
pub trait VisualDiffOps: OpsBase {
    async fn visual_diff_images(
        &self,
        before_artifact_id: &str,
        after_artifact_id: &str,
        mode: VisualDiffMode,
    ) -> Result<VisualDiffReport, Self::Error>;
    async fn visual_crop_region(
        &self,
        artifact_id: &str,
        rect: &CropRect,
    ) -> Result<ArtifactChunk, Self::Error>;
}

// ── Problems ──────────────────────────────────────────────────────────────────

#[async_trait]
pub trait ProblemReadOps: OpsBase {
    async fn problem_list(&self, project: &str) -> Result<Vec<ProblemSummary>, Self::Error>;
    async fn problem_list_detailed(&self, project: &str) -> Result<Vec<Problem>, Self::Error>;
    async fn problem_get(&self, id: &str) -> Result<Problem, Self::Error>;
}

#[async_trait]
pub trait ProblemWriteOps: OpsBase {
    async fn problem_create(
        &self,
        project: &str,
        form: &NewProblem,
    ) -> Result<Problem, Self::Error>;
    async fn problem_delete(&self, id: &str) -> Result<(), Self::Error>;
    async fn problem_triage(&self, id: &str, patch: &ProblemPatch) -> Result<Problem, Self::Error>;
    async fn problem_comment(&self, id: &str, form: &NewComment) -> Result<Comment, Self::Error>;
    async fn problem_analyze(
        &self,
        project: &str,
        capture: &str,
    ) -> Result<Vec<Problem>, Self::Error>;
}

#[async_trait]
pub trait ProblemSelectionOps: OpsBase {
    async fn problem_cite_selection(
        &self,
        id: &str,
        selection: &str,
    ) -> Result<Problem, Self::Error>;
    async fn problem_uncite_selection(
        &self,
        id: &str,
        selection: &str,
    ) -> Result<Problem, Self::Error>;
}

// ── Fixes & selections ────────────────────────────────────────────────────────

#[async_trait]
pub trait FixReadOps: OpsBase {
    async fn fix_list(&self, project: &str) -> Result<Vec<FixSummary>, Self::Error>;
    async fn fix_list_detailed(&self, project: &str) -> Result<Vec<Fix>, Self::Error>;
    async fn fix_get(&self, id: &str) -> Result<Fix, Self::Error>;
}

#[async_trait]
pub trait FixWriteOps: OpsBase {
    async fn fix_create(&self, project: &str, form: &NewFix) -> Result<Fix, Self::Error>;
    async fn fix_step(&self, id: &str, form: &NewStep) -> Result<Step, Self::Error>;
    async fn fix_review(&self, id: &str, form: &NewReview) -> Result<Fix, Self::Error>;
}

#[async_trait]
pub trait FixSelectionOps: OpsBase {
    async fn fix_cite_selection(
        &self,
        id: &str,
        selection: &str,
        link: &FixSelectionLink,
    ) -> Result<Fix, Self::Error>;
    async fn fix_uncite_selection(&self, id: &str, selection: &str) -> Result<Fix, Self::Error>;
}

#[async_trait]
pub trait SelectionOps: OpsBase {
    async fn selection_create(
        &self,
        project: &str,
        form: &NewSelection,
    ) -> Result<Selection, Self::Error>;
    async fn selection_get(&self, id: &str) -> Result<Selection, Self::Error>;
}

// ── Design & design systems ───────────────────────────────────────────────────

#[async_trait]
pub trait DesignLogOps: OpsBase {
    async fn design_list(&self, project: &str) -> Result<Vec<DesignEntry>, Self::Error>;
    async fn design_record(
        &self,
        project: &str,
        form: &NewDesignEntry,
    ) -> Result<DesignEntry, Self::Error>;
}

#[async_trait]
pub trait DesignSystemReadOps: OpsBase {
    async fn design_system_list(&self, project: &str) -> Result<SystemList, Self::Error>;
    async fn design_system_get(
        &self,
        project: &str,
        version: &str,
    ) -> Result<SystemVersion, Self::Error>;
}

#[async_trait]
pub trait DesignSystemWriteOps: OpsBase {
    async fn design_system_create(&self, project: &str) -> Result<SystemVersion, Self::Error>;
    async fn design_system_edit(
        &self,
        project: &str,
        version: &str,
        edit: &DocumentEdit,
    ) -> Result<SystemVersion, Self::Error>;
    async fn design_system_act(
        &self,
        project: &str,
        version: &str,
        form: &SystemActionForm,
    ) -> Result<SystemVersion, Self::Error>;
}

// ── Jobs, scenarios & topics ──────────────────────────────────────────────────

#[async_trait]
pub trait JobQueueOps: OpsBase {
    async fn job_list(&self, project: &str, open: bool) -> Result<Vec<Job>, Self::Error>;
    async fn job_get(&self, id: &str) -> Result<Job, Self::Error>;
    async fn job_create(&self, project: &str, form: &NewJob) -> Result<Job, Self::Error>;
    async fn job_cancel(&self, id: &str) -> Result<Job, Self::Error>;
}

#[async_trait]
pub trait JobWorkerOps: OpsBase {
    async fn job_claim(&self, project: &str) -> Result<Option<Job>, Self::Error>;
    async fn job_progress(&self, id: &str, progress: &JobProgress) -> Result<Job, Self::Error>;
    async fn job_complete(&self, id: &str, result: &JobResult) -> Result<Job, Self::Error>;
    async fn job_fail(&self, id: &str, failure: &JobFailure) -> Result<Job, Self::Error>;
}

#[async_trait]
pub trait ScenarioOps: OpsBase {
    async fn scenario_list(&self, project: &str) -> Result<Vec<ScenarioSummary>, Self::Error>;
    async fn scenario_get(
        &self,
        project: &str,
        name: &str,
        hash: Option<&str>,
    ) -> Result<ScenarioVersion, Self::Error>;
    async fn scenario_put(
        &self,
        project: &str,
        scenario: &Scenario,
    ) -> Result<ScenarioVersion, Self::Error>;
}

#[async_trait]
pub trait TopicReadOps: OpsBase {
    async fn topic_tree(&self, project: &str) -> Result<TopicTree, Self::Error>;
    async fn topic_captures(
        &self,
        id: &str,
        direct: bool,
    ) -> Result<Vec<CaptureSummary>, Self::Error>;
}

#[async_trait]
pub trait TopicWriteOps: OpsBase {
    async fn topic_create(&self, project: &str, form: &NewTopic) -> Result<Topic, Self::Error>;
    async fn topic_rename(&self, id: &str, form: &TopicRename) -> Result<Topic, Self::Error>;
    async fn topic_move(&self, id: &str, form: &TopicMove) -> Result<Topic, Self::Error>;
    async fn topic_delete(&self, id: &str, recursive: bool) -> Result<(), Self::Error>;
    async fn topic_file_capture(&self, id: &str, capture: &str) -> Result<(), Self::Error>;
    async fn topic_unfile_capture(&self, id: &str, capture: &str) -> Result<(), Self::Error>;
}

// ── Offline static analysis ───────────────────────────────────────────────────

#[async_trait]
pub trait OfflineScanOps: OpsBase {
    async fn offline_discover(&self, path: &str) -> Result<Value, Self::Error>;
    async fn offline_scan(
        &self,
        path: &str,
        framework: Option<&str>,
        include_tests: bool,
    ) -> Result<Value, Self::Error>;
}

#[async_trait]
pub trait OfflineCatalogOps: OpsBase {
    async fn offline_check(&self, path: &str) -> Result<Value, Self::Error>;
    async fn offline_rules(&self) -> Result<Value, Self::Error>;
    async fn offline_modules(&self) -> Result<Value, Self::Error>;
}
