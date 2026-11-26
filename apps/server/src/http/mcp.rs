//! The Model Context Protocol endpoint (`POST /mcp`): JSON-RPC 2.0 whose tools call the
//! same services as the REST API, so an agent has exactly the access its token gives.
//!
//! Tools are the variants of [`Call`]; [`catalog`] describes them. A test keeps the two
//! in step.

use crate::{
    domain::{ApiError, Principal},
    http::ops::{MAX_READ_BYTES, ServerOps},
    state::Shared,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rupicola_api::{
    CaptureRemoval, CaptureRunRequest, ContactSheetRequest, CropRect, DocumentEdit,
    FixSelectionLink, FrameSampleStrategy, JobFailure, JobProgress, JobResult, NewComment,
    NewDesignEntry, NewFinding, NewFix, NewJob, NewProblem, NewProject, NewReview, NewSelection,
    NewStep, NewTopic, ParentsEdit, ProblemPatch, ProjectUpdate, PublishCaptureRequest,
    RecaptureSettingsPatch, RecordClipRequest, Scenario, SnapshotCheckpointRequest,
    SystemActionForm, TopicMove, TopicRename, VisualDiffMode, WorkspaceOps,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Protocol revisions the server speaks, newest last.
const PROTOCOL_VERSIONS: &[&str] = &["2025-03-26", "2025-06-18", "2025-11-25"];
/// Revision answered when the client asks for one the server does not know.
const DEFAULT_PROTOCOL_VERSION: &str = "2025-11-25";
const SERVER_NAME: &str = "rupicola";
/// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
/// Bytes of an artifact one `artifact_read` returns unless asked for fewer.
const DEFAULT_READ_BYTES: usize = 256 * 1024;

fn default_read_bytes() -> usize {
    DEFAULT_READ_BYTES
}

fn default_dot() -> String {
    ".".to_owned()
}

/// One tool invocation: the tool's name and its arguments.
/// Every variant maps 1:1 to a [`WorkspaceOps`] method.
#[derive(Deserialize)]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
enum Call {
    Me {},
    ProjectList {},
    ProjectGet {
        id: String,
    },
    ProjectCreate(NewProject),
    ProjectUpdate {
        id: String,
        #[serde(flatten)]
        form: ProjectUpdate,
    },
    ProjectDelete {
        id: String,
    },
    ProjectLimits {
        project: String,
    },
    CaptureSettingsGet {
        project: String,
    },
    CaptureSettingsUpdate {
        project: String,
        #[serde(flatten)]
        patch: RecaptureSettingsPatch,
    },
    CaptureList {
        project: String,
    },
    CaptureSearch {
        project: String,
        #[serde(default)]
        query: Option<String>,
    },
    CaptureGet {
        id: String,
    },
    CaptureCapabilities {},
    CapturePublish(PublishCaptureRequest),
    CaptureReview {
        id: String,
        #[serde(flatten)]
        finding: NewFinding,
    },
    CaptureCitations {
        id: String,
    },
    CaptureDelete {
        id: String,
        #[serde(flatten)]
        form: CaptureRemoval,
    },
    CaptureLineage {
        id: String,
        #[serde(default)]
        depth: Option<u32>,
    },
    CaptureParentsGet {
        id: String,
    },
    CaptureParentsEdit {
        id: String,
        #[serde(flatten)]
        form: ParentsEdit,
    },
    CaptureCompare {
        before: String,
        after: String,
    },
    ArtifactRead {
        id: String,
        #[serde(default)]
        offset: u64,
        #[serde(default = "default_read_bytes")]
        length: usize,
    },
    ProblemList {
        project: String,
    },
    ProblemListDetailed {
        project: String,
    },
    ProblemGet {
        id: String,
    },
    ProblemCreate {
        project: String,
        #[serde(flatten)]
        problem: NewProblem,
    },
    ProblemDelete {
        id: String,
    },
    ProblemTriage {
        id: String,
        #[serde(flatten)]
        patch: ProblemPatch,
    },
    ProblemComment {
        id: String,
        #[serde(flatten)]
        comment: NewComment,
    },
    ProblemAnalyze {
        project: String,
        capture: String,
    },
    ProblemCiteSelection {
        id: String,
        selection: String,
    },
    ProblemUnciteSelection {
        id: String,
        selection: String,
    },
    FixList {
        project: String,
    },
    FixListDetailed {
        project: String,
    },
    FixGet {
        id: String,
    },
    FixCreate {
        project: String,
        #[serde(flatten)]
        fix: NewFix,
    },
    FixStep {
        id: String,
        #[serde(flatten)]
        step: NewStep,
    },
    FixReview {
        id: String,
        #[serde(flatten)]
        review: NewReview,
    },
    FixCiteSelection {
        id: String,
        selection: String,
        #[serde(flatten)]
        link: FixSelectionLink,
    },
    FixUnciteSelection {
        id: String,
        selection: String,
    },
    DesignList {
        project: String,
    },
    DesignRecord {
        project: String,
        #[serde(flatten)]
        entry: NewDesignEntry,
    },
    DesignSystemList {
        project: String,
    },
    DesignSystemGet {
        project: String,
        version: String,
    },
    DesignSystemCreate {
        project: String,
    },
    DesignSystemEdit {
        project: String,
        version: String,
        #[serde(flatten)]
        edit: Box<DocumentEdit>,
    },
    DesignSystemAct {
        project: String,
        version: String,
        #[serde(flatten)]
        form: SystemActionForm,
    },
    JobList {
        project: String,
        #[serde(default)]
        open: bool,
    },
    JobGet {
        id: String,
    },
    JobCreate {
        project: String,
        #[serde(flatten)]
        job: NewJob,
    },
    JobClaim {
        project: String,
    },
    JobProgress {
        id: String,
        #[serde(flatten)]
        progress: JobProgress,
    },
    JobComplete {
        id: String,
        #[serde(flatten)]
        result: JobResult,
    },
    JobFail {
        id: String,
        #[serde(flatten)]
        failure: JobFailure,
    },
    JobCancel {
        id: String,
    },
    ScenarioList {
        project: String,
    },
    ScenarioGet {
        project: String,
        name: String,
        #[serde(default)]
        hash: Option<String>,
    },
    ScenarioPut {
        project: String,
        scenario: Scenario,
    },
    SelectionCreate {
        project: String,
        #[serde(flatten)]
        form: NewSelection,
    },
    SelectionGet {
        id: String,
    },
    TopicTree {
        project: String,
    },
    TopicCreate {
        project: String,
        #[serde(flatten)]
        form: NewTopic,
    },
    TopicRename {
        id: String,
        #[serde(flatten)]
        form: TopicRename,
    },
    TopicMove {
        id: String,
        #[serde(flatten)]
        form: TopicMove,
    },
    TopicDelete {
        id: String,
        #[serde(default)]
        recursive: bool,
    },
    TopicCaptures {
        id: String,
        #[serde(default)]
        direct: bool,
    },
    TopicFileCapture {
        id: String,
        capture: String,
    },
    TopicUnfileCapture {
        id: String,
        capture: String,
    },
    OfflineDiscover {
        #[serde(default = "default_dot")]
        path: String,
    },
    OfflineScan {
        #[serde(default = "default_dot")]
        path: String,
        #[serde(default)]
        framework: Option<String>,
        #[serde(default)]
        include_tests: bool,
    },
    OfflineCheck {
        #[serde(default = "default_dot")]
        path: String,
    },
    OfflineRules {},
    OfflineModules {},
    CaptureRunScenario(CaptureRunRequest),
    CaptureRecordClip(RecordClipRequest),
    CaptureSnapshotCheckpoint(SnapshotCheckpointRequest),
    MediaProbe {
        artifact_id: String,
    },
    MediaMotionDiff {
        artifact_id: String,
        #[serde(default = "default_motion_threshold")]
        threshold: f64,
    },
    MediaContactSheet {
        artifact_id: String,
        #[serde(flatten)]
        req: ContactSheetRequest,
    },
    MediaSampleFrames {
        artifact_id: String,
        #[serde(flatten)]
        strategy: FrameSampleStrategy,
    },
    VisualDiffImages {
        before_artifact_id: String,
        after_artifact_id: String,
        #[serde(default = "default_diff_mode")]
        mode: VisualDiffMode,
    },
    VisualCropRegion {
        artifact_id: String,
        #[serde(flatten)]
        rect: CropRect,
    },
}

fn default_motion_threshold() -> f64 {
    0.05
}

const fn default_diff_mode() -> VisualDiffMode {
    VisualDiffMode::Highlight
}

/// A handler's JSON body, as a value.
fn value<T: Serialize>(body: &T) -> Result<Value, ApiError> {
    Ok(serde_json::to_value(body).map_err(anyhow::Error::from)?)
}

impl Call {
    async fn run(self, state: Shared, principal: Principal) -> Result<Value, ApiError> {
        let ops = ServerOps::new(state, principal);
        self.run_on(&ops).await
    }

    async fn run_on<T: WorkspaceOps<Error = ApiError>>(self, ops: &T) -> Result<Value, ApiError> {
        match self {
            Self::Me {} => value(&ops.me().await?),
            Self::ProjectList {} => value(&ops.project_list().await?),
            Self::ProjectGet { id } => value(&ops.project_get(&id).await?),
            Self::ProjectCreate(form) => value(&ops.project_create(&form).await?),
            Self::ProjectUpdate { id, form } => value(&ops.project_update(&id, &form).await?),
            Self::ProjectDelete { id } => {
                ops.project_delete(&id).await?;
                Ok(json!({"deleted": id}))
            }
            Self::ProjectLimits { project } => value(&ops.project_limits(&project).await?),
            Self::CaptureSettingsGet { project } => {
                value(&ops.capture_settings_get(&project).await?)
            }
            Self::CaptureSettingsUpdate { project, patch } => {
                value(&ops.capture_settings_update(&project, &patch).await?)
            }
            Self::CaptureList { project } => value(&ops.capture_list(&project).await?),
            Self::CaptureSearch { project, query } => {
                value(&ops.capture_search(&project, query.as_deref()).await?)
            }
            Self::CaptureGet { id } => value(&ops.capture_get(&id).await?),
            Self::CaptureCapabilities {} => value(&ops.capture_capabilities().await?),
            Self::CapturePublish(req) => value(&ops.capture_publish(&req).await?),
            Self::CaptureReview { id, finding } => value(&ops.capture_review(&id, &finding).await?),
            Self::CaptureCitations { id } => value(&ops.capture_citations(&id).await?),
            Self::CaptureDelete { id, form } => {
                ops.capture_delete(&id, &form).await?;
                Ok(json!({"deleted": id}))
            }
            Self::CaptureLineage { id, depth } => value(&ops.capture_lineage(&id, depth).await?),
            Self::CaptureParentsGet { id } => value(&ops.capture_parents_get(&id).await?),
            Self::CaptureParentsEdit { id, form } => {
                value(&ops.capture_parents_edit(&id, &form).await?)
            }
            Self::CaptureCompare { before, after } => {
                value(&ops.capture_compare(&before, &after).await?)
            }
            Self::ArtifactRead { id, offset, length } => {
                value(&ops.artifact_read(&id, offset, length).await?)
            }
            other => other.run_domain(ops).await,
        }
    }

    async fn run_domain<T: WorkspaceOps<Error = ApiError>>(
        self,
        ops: &T,
    ) -> Result<Value, ApiError> {
        match self {
            Self::ProblemList { project } => value(&ops.problem_list(&project).await?),
            Self::ProblemListDetailed { project } => {
                value(&ops.problem_list_detailed(&project).await?)
            }
            Self::ProblemGet { id } => value(&ops.problem_get(&id).await?),
            Self::ProblemCreate { project, problem } => {
                value(&ops.problem_create(&project, &problem).await?)
            }
            Self::ProblemDelete { id } => {
                ops.problem_delete(&id).await?;
                Ok(json!({"deleted": id}))
            }
            Self::ProblemTriage { id, patch } => value(&ops.problem_triage(&id, &patch).await?),
            Self::ProblemComment { id, comment } => {
                value(&ops.problem_comment(&id, &comment).await?)
            }
            Self::ProblemAnalyze { project, capture } => {
                value(&ops.problem_analyze(&project, &capture).await?)
            }
            Self::ProblemCiteSelection { id, selection } => {
                value(&ops.problem_cite_selection(&id, &selection).await?)
            }
            Self::ProblemUnciteSelection { id, selection } => {
                value(&ops.problem_uncite_selection(&id, &selection).await?)
            }
            Self::FixList { project } => value(&ops.fix_list(&project).await?),
            Self::FixListDetailed { project } => value(&ops.fix_list_detailed(&project).await?),
            Self::FixGet { id } => value(&ops.fix_get(&id).await?),
            Self::FixCreate { project, fix } => value(&ops.fix_create(&project, &fix).await?),
            Self::FixStep { id, step } => value(&ops.fix_step(&id, &step).await?),
            Self::FixReview { id, review } => value(&ops.fix_review(&id, &review).await?),
            Self::FixCiteSelection {
                id,
                selection,
                link,
            } => value(&ops.fix_cite_selection(&id, &selection, &link).await?),
            Self::FixUnciteSelection { id, selection } => {
                value(&ops.fix_uncite_selection(&id, &selection).await?)
            }
            Self::DesignList { project } => value(&ops.design_list(&project).await?),
            Self::DesignRecord { project, entry } => {
                value(&ops.design_record(&project, &entry).await?)
            }
            Self::DesignSystemList { project } => value(&ops.design_system_list(&project).await?),
            Self::DesignSystemGet { project, version } => {
                value(&ops.design_system_get(&project, &version).await?)
            }
            Self::DesignSystemCreate { project } => {
                value(&ops.design_system_create(&project).await?)
            }
            Self::DesignSystemEdit {
                project,
                version,
                edit,
            } => value(&ops.design_system_edit(&project, &version, &edit).await?),
            Self::DesignSystemAct {
                project,
                version,
                form,
            } => value(&ops.design_system_act(&project, &version, &form).await?),
            other => other.run_ops_and_offline(ops).await,
        }
    }

    async fn run_ops_and_offline<T: WorkspaceOps<Error = ApiError>>(
        self,
        ops: &T,
    ) -> Result<Value, ApiError> {
        match self {
            Self::JobList { project, open } => value(&ops.job_list(&project, open).await?),
            Self::JobGet { id } => value(&ops.job_get(&id).await?),
            Self::JobCreate { project, job } => value(&ops.job_create(&project, &job).await?),
            Self::JobClaim { project } => value(&ops.job_claim(&project).await?),
            Self::JobProgress { id, progress } => value(&ops.job_progress(&id, &progress).await?),
            Self::JobComplete { id, result } => value(&ops.job_complete(&id, &result).await?),
            Self::JobFail { id, failure } => value(&ops.job_fail(&id, &failure).await?),
            Self::JobCancel { id } => value(&ops.job_cancel(&id).await?),
            Self::ScenarioList { project } => value(&ops.scenario_list(&project).await?),
            Self::ScenarioGet {
                project,
                name,
                hash,
            } => value(&ops.scenario_get(&project, &name, hash.as_deref()).await?),
            Self::ScenarioPut { project, scenario } => {
                value(&ops.scenario_put(&project, &scenario).await?)
            }
            Self::SelectionCreate { project, form } => {
                value(&ops.selection_create(&project, &form).await?)
            }
            Self::SelectionGet { id } => value(&ops.selection_get(&id).await?),
            Self::TopicTree { project } => value(&ops.topic_tree(&project).await?),
            Self::TopicCreate { project, form } => value(&ops.topic_create(&project, &form).await?),
            Self::TopicRename { id, form } => value(&ops.topic_rename(&id, &form).await?),
            Self::TopicMove { id, form } => value(&ops.topic_move(&id, &form).await?),
            Self::TopicDelete { id, recursive } => {
                ops.topic_delete(&id, recursive).await?;
                Ok(json!({"deleted": id}))
            }
            Self::TopicCaptures { id, direct } => value(&ops.topic_captures(&id, direct).await?),
            Self::TopicFileCapture { id, capture } => {
                ops.topic_file_capture(&id, &capture).await?;
                Ok(json!({"filed": capture, "topic": id}))
            }
            Self::TopicUnfileCapture { id, capture } => {
                ops.topic_unfile_capture(&id, &capture).await?;
                Ok(json!({"unfiled": capture, "topic": id}))
            }
            Self::OfflineDiscover { path } => ops.offline_discover(&path).await,
            Self::OfflineScan {
                path,
                framework,
                include_tests,
            } => {
                ops.offline_scan(&path, framework.as_deref(), include_tests)
                    .await
            }
            Self::OfflineCheck { path } => ops.offline_check(&path).await,
            Self::OfflineRules {} => ops.offline_rules().await,
            Self::OfflineModules {} => ops.offline_modules().await,
            Self::CaptureRunScenario(req) => value(&ops.capture_run_scenario(&req).await?),
            Self::CaptureRecordClip(req) => value(&ops.capture_record_clip(&req).await?),
            Self::CaptureSnapshotCheckpoint(req) => {
                value(&ops.capture_snapshot_checkpoint(&req).await?)
            }
            Self::MediaProbe { artifact_id } => value(&ops.media_probe(&artifact_id).await?),
            Self::MediaMotionDiff {
                artifact_id,
                threshold,
            } => value(&ops.media_motion_diff(&artifact_id, threshold).await?),
            Self::MediaContactSheet { artifact_id, req } => {
                value(&ops.media_contact_sheet(&artifact_id, &req).await?)
            }
            Self::MediaSampleFrames {
                artifact_id,
                strategy,
            } => value(&ops.media_sample_frames(&artifact_id, &strategy).await?),
            Self::VisualDiffImages {
                before_artifact_id,
                after_artifact_id,
                mode,
            } => value(
                &ops.visual_diff_images(&before_artifact_id, &after_artifact_id, mode)
                    .await?,
            ),
            Self::VisualCropRegion { artifact_id, rect } => {
                value(&ops.visual_crop_region(&artifact_id, &rect).await?)
            }
            _ => unreachable!("handled by run_on / run_domain"),
        }
    }
}

/// One tool's catalog entry.
struct Tool {
    name: &'static str,
    description: &'static str,
    required: &'static [&'static str],
    properties: fn() -> Value,
}

impl Tool {
    fn describe(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": {
                "type": "object",
                "required": self.required,
                "properties": (self.properties)(),
            },
        })
    }
}

fn text() -> Value {
    json!({"type": "string"})
}

fn string_list() -> Value {
    json!({"type": "array", "items": {"type": "string"}})
}

fn severity() -> Value {
    json!({"enum": ["critical", "major", "minor", "polish"]})
}

fn comparisons() -> Value {
    json!({"type": "array", "items": {"type": "object",
        "required": ["label", "before", "after"],
        "properties": {"label": text(), "route": text(), "viewport": text(), "theme": text(),
            "before": text(), "after": text(), "diff": text(),
            "changed_ratio": {"type": "number"},
            "view": {"enum": ["isolated", "integrated"],
                "description": "isolated: the base against the base plus this fix alone. integrated: the base with every approved fix against the same plus this fix."},
            "base_revision": text(), "revision": text(),
            "applied_fixes": {"type": "array", "items": text(),
                "description": "Ids of the approved fixes the before tree already had; empty when isolated."}}}})
}

fn step_properties() -> Value {
    json!({"title": text(), "detail": text(), "verification": text(), "evidence": text()})
}

fn selections_schema() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "required": ["label", "items"],
            "properties": {
                "label": text(),
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["capture", "mark"],
                        "properties": {
                            "capture": text(),
                            "artifact": text(),
                            "path": text(),
                            "mark": {"enum": ["proof", "context"]},
                            "fragment": {"type": "object"}
                        }
                    }
                }
            }
        }
    })
}

/// Every tool, in the exact order of `rupicola_api::WORKSPACE_OP_NAMES`.
const TOOLS: &[Tool] = &[
    // Identity & projects
    Tool {
        name: "me",
        description: "Show the identity and permissions of the authenticated credential",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "project_list",
        description: "List the projects this credential can see",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "project_get",
        description: "Read one project's metadata",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "project_create",
        description: "Create a project with a stable slug id and a label",
        required: &["id", "label"],
        properties: || json!({"id": text(), "label": text()}),
    },
    Tool {
        name: "project_update",
        description: "Rename a project's label",
        required: &["id", "label"],
        properties: || json!({"id": text(), "label": text()}),
    },
    Tool {
        name: "project_delete",
        description: "Permanently delete a project",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "project_limits",
        description: "Read the effective upload and capture limits for a project",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "capture_settings_get",
        description: "Read a project's capture settings (auto_recapture, base_ref)",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "capture_settings_update",
        description: "Update a project's capture settings (auto_recapture, base_ref)",
        required: &["project"],
        properties: || json!({"project": text(), "auto_recapture": {"type": ["boolean", "null"]}, "base_ref": {"type": ["string", "null"]}}),
    },
    // Captures & artifacts
    Tool {
        name: "capture_list",
        description: "List a project's captures, newest first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "capture_search",
        description: "Search a project's captures with query parameters (e.g. include=details&limit=20&scenario=home)",
        required: &["project"],
        properties: || json!({"project": text(), "query": text()}),
    },
    Tool {
        name: "capture_get",
        description: "One capture with its checkpoints, clips and review notes",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "capture_capabilities",
        description: "Probe which host OS capture backends are available and return install hints for missing ones",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "capture_publish",
        description: "Publish a capture and its PNG/MP4 media blobs directly through MCP, returning stored artifact IDs for problem_create and fix_create. Pass `data_base64` per member (or `file_path` / `bundle_dir` when connected via `rupicola mcp-bridge`).",
        required: &["project", "scenario", "members"],
        properties: || {
            json!({
                "project": text(),
                "scenario": text(),
                "revision": text(),
                "viewport": text(),
                "theme": text(),
                "bundle_dir": text(),
                "members": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": text(),
                            "role": {"enum": ["checkpoint", "clip-video", "clip-frame", "clip-sheet", "clip-analysis", "ui-snapshot"]},
                            "mime": text(),
                            "path": text(),
                            "data_base64": text(),
                            "file_path": text(),
                            "sha256": text(),
                            "bytes": {"type": "integer", "minimum": 1}
                        }
                    }
                }
            })
        },
    },
    Tool {
        name: "capture_review",
        description: "Leave a review note on a capture, optionally pointing at one of its artifacts",
        required: &["id", "note"],
        properties: || json!({"id": text(), "note": text(), "evidence_artifact_id": text()}),
    },
    Tool {
        name: "capture_citations",
        description: "List what cites a capture, its children, and whether removal is allowed (with confirm token when cited)",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "capture_delete",
        description: "Remove a capture, leaving a tombstone; pass `confirm` from capture_citations when cited",
        required: &["id", "reason"],
        properties: || json!({"id": text(), "reason": text(), "confirm": text()}),
    },
    Tool {
        name: "capture_lineage",
        description: "Read the lineage graph around a capture up to `depth` steps",
        required: &["id"],
        properties: || json!({"id": text(), "depth": {"type": "integer", "minimum": 1}}),
    },
    Tool {
        name: "capture_parents_get",
        description: "Read a capture's declared parents, generation, and edit history",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "capture_parents_edit",
        description: "Replace a capture's parents using optimistic concurrency (`generation`)",
        required: &["id", "generation", "parents", "reason"],
        properties: || {
            json!({"id": text(), "generation": {"type": "integer"}, "reason": text(),
            "parents": {"type": "array", "items": {"type": "object", "required": ["id", "kind"],
                "properties": {"id": text(), "kind": {"enum": ["baseline", "input"]}}}}})
        },
    },
    Tool {
        name: "capture_compare",
        description: "Compare two captures of the same project to see added, removed, and changed members",
        required: &["before", "after"],
        properties: || json!({"before": text(), "after": text()}),
    },
    Tool {
        name: "artifact_read",
        description: "Read a slice of a captured file as base64; page with offset and length",
        required: &["id"],
        properties: || json!({"id": text(), "offset": {"type": "integer", "minimum": 0}, "length": {"type": "integer", "minimum": 1, "maximum": MAX_READ_BYTES}}),
    },
    // Problems
    Tool {
        name: "problem_list",
        description: "List a project's problems, most recently updated first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "problem_list_detailed",
        description: "List every problem of a project with full rules, evidence, selections and comments",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "problem_get",
        description: "One problem with its rules, evidence and comments",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "problem_create",
        description: "File a problem; requires at least one evidence artifact id from the project's captures",
        required: &["project", "title", "severity", "origin", "evidence"],
        properties: || {
            json!({"project": text(), "title": text(), "severity": severity(),
        "origin": {"enum": ["agent-measured", "agent-visual", "user-reported"]},
        "route": text(), "viewport": text(), "theme": text(), "diagnosis": text(), "proposed_solution": text(),
        "rules": {"type": "array", "items": {"type": "object", "required": ["scope", "code", "title"], "properties": {"scope": text(), "code": text(), "title": text()}}},
        "evidence": {"type": "array", "minItems": 1, "items": text()}, "selections": selections_schema()})
        },
    },
    Tool {
        name: "problem_delete",
        description: "Permanently delete a problem from its project",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "problem_triage",
        description: "Change a problem's status, severity or proposed solution",
        required: &["id"],
        properties: || {
            json!({"id": text(),
        "status": {"enum": ["open", "in-progress", "intentional", "false-positive", "needs-reprocess", "resolved"]},
        "severity": severity(), "proposed_solution": text()})
        },
    },
    Tool {
        name: "problem_comment",
        description: "Add a comment to a problem",
        required: &["id", "text"],
        properties: || json!({"id": text(), "text": text()}),
    },
    Tool {
        name: "problem_analyze",
        description: "Infer and file deduplicated, unsuppressed problems from a capture's findings",
        required: &["project", "capture"],
        properties: || json!({"project": text(), "capture": text()}),
    },
    Tool {
        name: "problem_cite_selection",
        description: "Attach an existing selection to a problem",
        required: &["id", "selection"],
        properties: || json!({"id": text(), "selection": text()}),
    },
    Tool {
        name: "problem_uncite_selection",
        description: "Detach a selection from a problem",
        required: &["id", "selection"],
        properties: || json!({"id": text(), "selection": text()}),
    },
    // Fixes
    Tool {
        name: "fix_list",
        description: "List a project's fixes, most recently updated first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "fix_list_detailed",
        description: "List every fix of a project with comparisons, selections, steps and reviews",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "fix_get",
        description: "One fix with its comparisons, steps and reviews",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "fix_create",
        description: "File a fix for one or more problems; they go in progress until the fix is approved",
        required: &["project", "title", "problems"],
        properties: || {
            json!({"project": text(), "title": text(), "summary": text(), "git_ref": text(),
        "problems": string_list(), "tactics": string_list(), "files": string_list(),
        "comparisons": comparisons(), "baseline": selections_schema(), "proof": selections_schema(),
        "steps": {"type": "array", "items": {"type": "object", "required": ["title"], "properties": step_properties()}}})
        },
    },
    Tool {
        name: "fix_step",
        description: "Record one more verified step of a fix",
        required: &["id", "title"],
        properties: || {
            let mut properties = step_properties();
            properties["id"] = text();
            properties
        },
    },
    Tool {
        name: "fix_review",
        description: "Approve a fix (resolves its problems), request changes, or mark it superseded",
        required: &["id", "outcome"],
        properties: || {
            json!({"id": text(),
        "outcome": {"enum": ["approved", "changes-requested", "superseded"]}, "comment": text()})
        },
    },
    Tool {
        name: "fix_cite_selection",
        description: "Attach an existing selection to a fix as baseline or proof",
        required: &["id", "selection", "role"],
        properties: || json!({"id": text(), "selection": text(), "role": {"enum": ["baseline", "proof"]}}),
    },
    Tool {
        name: "fix_uncite_selection",
        description: "Detach a selection from a fix",
        required: &["id", "selection"],
        properties: || json!({"id": text(), "selection": text()}),
    },
    // Design & design systems
    Tool {
        name: "design_list",
        description: "List a project's design records (briefs, directions, tactics, decisions), newest first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "design_record",
        description: "Record a design brief, direction, tactic or decision; supersedes names the record it replaces",
        required: &["project", "kind", "status", "title"],
        properties: || {
            json!({"project": text(),
        "kind": {"enum": ["brief", "direction", "tactic", "decision"]},
        "status": {"enum": ["proposed", "adopted", "rejected"]},
        "title": text(), "note": text(), "surface": text(), "supersedes": text(),
        "reference": text(), "evidence": text()})
        },
    },
    Tool {
        name: "design_system_list",
        description: "List a project's design-system versions and which one is adopted",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "design_system_get",
        description: "Read one design-system version document (pass `adopted` or a version id)",
        required: &["project", "version"],
        properties: || json!({"project": text(), "version": text()}),
    },
    Tool {
        name: "design_system_create",
        description: "Create a new draft design-system version starting from the adopted version",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "design_system_edit",
        description: "Replace the document of a draft design-system version with optimistic concurrency (`generation`)",
        required: &["project", "version", "generation", "document"],
        properties: || json!({"project": text(), "version": text(), "generation": {"type": "integer"}, "document": {"type": "object"}}),
    },
    Tool {
        name: "design_system_act",
        description: "Validate, review, adopt, or vary a design-system version",
        required: &["project", "version", "action", "generation"],
        properties: || {
            json!({"project": text(), "version": text(),
            "action": {"enum": ["validate", "review", "adopt", "vary"]},
            "generation": {"type": "integer"}, "target": text(), "note": text()})
        },
    },
    // Jobs (including runner lifecycle)
    Tool {
        name: "job_list",
        description: "List a project's capture jobs, newest first; open=true keeps the ones waiting or running",
        required: &["project"],
        properties: || json!({"project": text(), "open": {"type": "boolean"}}),
    },
    Tool {
        name: "job_get",
        description: "One capture job: status, cells_done of cells_total, the runner, and the captures it made",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "job_create",
        description: "Queue a capture job: run a scenario on one or more trees of the code. A runner takes it, builds each tree (a git ref with other refs merged on top), captures the scenario's matrix and uploads the captures. Poll job_get for cells_done of cells_total",
        required: &["project", "scenario", "trees"],
        properties: || {
            json!({"project": text(), "scenario": text(),
                "hash": {"type": "string", "description": "Exact scenario version; the latest when absent."},
                "fix": {"type": "string", "description": "The fix the captures are for."},
                "trees": {"type": "array", "items": {"type": "object", "required": ["name", "base"],
                    "properties": {"name": text(), "base": {"type": "string", "description": "Git ref to check out."},
                        "merge": string_list(), "fixes": string_list()}}}})
        },
    },
    Tool {
        name: "job_claim",
        description: "Claim the oldest queued capture job for a project (returns null when queue is empty)",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "job_progress",
        description: "Report runner progress on a claimed job and renew its lease",
        required: &["id", "cells_done"],
        properties: || json!({"id": text(), "cells_done": {"type": "integer", "minimum": 0}}),
    },
    Tool {
        name: "job_complete",
        description: "Finish a claimed job with the captures it produced",
        required: &["id", "captures"],
        properties: || json!({"id": text(), "captures": {"type": "array", "items": {"type": "object"}}}),
    },
    Tool {
        name: "job_fail",
        description: "Mark a claimed job as failed with a reason",
        required: &["id", "error"],
        properties: || json!({"id": text(), "error": text()}),
    },
    Tool {
        name: "job_cancel",
        description: "Cancel a capture job that has not finished; a runner holding it stops at its next report",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    // Scenarios
    Tool {
        name: "scenario_list",
        description: "List a project's scenarios, the capture definitions its repository owns, latest version of each",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "scenario_get",
        description: "One scenario definition: its latest version, or the exact version with `hash`",
        required: &["project", "name"],
        properties: || json!({"project": text(), "name": text(), "hash": text()}),
    },
    Tool {
        name: "scenario_put",
        description: "Push a scenario definition. The repository's TOML file is the source of truth; this keeps a copy under the hash of the parsed definition. Same definition twice is a no-op",
        required: &["project", "scenario"],
        properties: || {
            json!({"project": text(),
                "scenario": {"type": "object", "required": ["name", "url"],
                    "description": "The parsed .rupicola/scenarios/*.toml: name, url, environment, matrix {viewports, themes}, themes, steps, checks.",
                    "properties": {"name": text(), "url": text(), "environment": {"type": "object"},
                        "matrix": {"type": "object"}, "themes": {"type": "object"},
                        "steps": {"type": "array"}, "checks": {"type": "object"}}}})
        },
    },
    // Selections
    Tool {
        name: "selection_create",
        description: "Create a standalone selection of capture artifacts/fragments in a project",
        required: &["project", "label", "items"],
        properties: || {
            json!({"project": text(), "label": text(),
            "items": {"type": "array", "items": {"type": "object", "required": ["capture", "mark"],
                "properties": {"capture": text(), "artifact": text(), "path": text(),
                    "mark": {"enum": ["proof", "context"]}, "fragment": {"type": "object"}}}}})
        },
    },
    Tool {
        name: "selection_get",
        description: "Read one selection by id",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    // Topics
    Tool {
        name: "topic_tree",
        description: "Read a project's hierarchical topic tree",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "topic_create",
        description: "Create a topic path (like mkdir -p) in a project",
        required: &["project", "path"],
        properties: || json!({"project": text(), "path": text()}),
    },
    Tool {
        name: "topic_rename",
        description: "Rename a topic with optimistic concurrency (`generation`)",
        required: &["id", "name", "generation"],
        properties: || json!({"id": text(), "name": text(), "generation": {"type": "integer"}}),
    },
    Tool {
        name: "topic_move",
        description: "Move a topic under a new parent (or null for root) with optimistic concurrency (`generation`)",
        required: &["id", "generation"],
        properties: || json!({"id": text(), "parent": {"type": ["string", "null"]}, "generation": {"type": "integer"}}),
    },
    Tool {
        name: "topic_delete",
        description: "Delete a topic; pass recursive=true to delete subtopics as well",
        required: &["id"],
        properties: || json!({"id": text(), "recursive": {"type": "boolean"}}),
    },
    Tool {
        name: "topic_captures",
        description: "List captures filed under a topic (direct=true excludes subtopics)",
        required: &["id"],
        properties: || json!({"id": text(), "direct": {"type": "boolean"}}),
    },
    Tool {
        name: "topic_file_capture",
        description: "File a capture under a topic",
        required: &["id", "capture"],
        properties: || json!({"id": text(), "capture": text()}),
    },
    Tool {
        name: "topic_unfile_capture",
        description: "Remove a capture from a topic",
        required: &["id", "capture"],
        properties: || json!({"id": text(), "capture": text()}),
    },
    // Offline static analysis (rupicola-core; executed locally by rupicola mcp-bridge)
    Tool {
        name: "offline_discover",
        description: "Discover Rust UI framework dependencies in a local workspace without building",
        required: &[],
        properties: || json!({"path": text()}),
    },
    Tool {
        name: "offline_scan",
        description: "Run static Rust UI syntax scan against a local path",
        required: &[],
        properties: || json!({"path": text(), "framework": text(), "include_tests": {"type": "boolean"}}),
    },
    Tool {
        name: "offline_check",
        description: "Validate skill catalog Markdown files and links at a local path",
        required: &[],
        properties: || json!({"path": text()}),
    },
    Tool {
        name: "offline_rules",
        description: "List the supported static UI scan rules",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "offline_modules",
        description: "List available framework and UX reference module paths",
        required: &[],
        properties: || json!({}),
    },
    // Capture execution (via rupicola-capture)
    Tool {
        name: "capture_run_scenario",
        description: "Execute a stored project scenario via rupicola-capture (Chromium CDP) and publish the capture",
        required: &["project", "scenario", "base_url"],
        properties: || json!({"project": text(), "scenario": text(), "base_url": text(), "viewport": text(), "theme": text()}),
    },
    Tool {
        name: "capture_record_clip",
        description: "Record a video clip of a URL via rupicola-capture (Chromium CDP) and publish the capture",
        required: &["project", "url", "fps", "duration_ms"],
        properties: || {
            json!({"project": text(), "url": text(), "fps": {"type": "integer", "minimum": 1, "maximum": 60},
            "duration_ms": {"type": "integer", "minimum": 50}, "viewport": text(), "theme": text()})
        },
    },
    Tool {
        name: "capture_snapshot_checkpoint",
        description: "Capture a single checkpoint PNG (+ optional UI snapshot) via rupicola-capture (Chromium CDP)",
        required: &["project", "url", "label"],
        properties: || {
            json!({"project": text(), "url": text(), "label": text(),
            "include_ui_snapshot": {"type": "boolean"}, "viewport": text(), "theme": text()})
        },
    },
    // Progressive token-efficient video/media inspection
    Tool {
        name: "media_probe",
        description: "Probe image or video metadata (dimensions, duration, fps, frame_count, codec) with zero images in context",
        required: &["artifact_id"],
        properties: || json!({"artifact_id": text()}),
    },
    Tool {
        name: "media_motion_diff",
        description: "Detect frame-to-frame pixel/luminance spikes (flashes, flickers, jumps) numerically as pure JSON before opening PNGs",
        required: &["artifact_id"],
        properties: || json!({"artifact_id": text(), "threshold": {"type": "number", "minimum": 0.0, "maximum": 1.0}}),
    },
    Tool {
        name: "media_contact_sheet",
        description: "Render one cols x rows contact-sheet PNG for single-image visual triage of a video clip",
        required: &["artifact_id", "cols", "rows"],
        properties: || {
            json!({"artifact_id": text(), "cols": {"type": "integer", "minimum": 1, "maximum": 10},
            "rows": {"type": "integer", "minimum": 1, "maximum": 10},
            "start_ms": {"type": "integer", "minimum": 0}, "end_ms": {"type": "integer", "minimum": 1}})
        },
    },
    Tool {
        name: "media_sample_frames",
        description: "Progressively extract video frames (strategy: coarse | bisect | window) without re-reading already-inspected frames",
        required: &["artifact_id", "strategy"],
        properties: || {
            json!({"artifact_id": text(),
            "strategy": {"enum": ["coarse", "bisect", "window"]},
            "count": {"type": "integer", "minimum": 1, "maximum": 60},
            "start_ms": {"type": "integer", "minimum": 0},
            "end_ms": {"type": "integer", "minimum": 1},
            "fps": {"type": "integer", "minimum": 1, "maximum": 60},
            "exclude_frames": {"type": "array", "items": {"type": "integer", "minimum": 0}}})
        },
    },
    // Visual diff & targeted crop
    Tool {
        name: "visual_diff_images",
        description: "Compare two PNG artifacts (highlight | heatmap | side_by_side), store the diff artifact, and return changed_ratio + changed_regions",
        required: &["before_artifact_id", "after_artifact_id"],
        properties: || {
            json!({"before_artifact_id": text(), "after_artifact_id": text(),
            "mode": {"enum": ["highlight", "heatmap", "side_by_side"]}})
        },
    },
    Tool {
        name: "visual_crop_region",
        description: "Crop a rectangular region from a PNG artifact to inspect details without spending tokens on the full screen",
        required: &["artifact_id", "x", "y", "width", "height"],
        properties: || {
            json!({"artifact_id": text(),
            "x": {"type": "integer", "minimum": 0}, "y": {"type": "integer", "minimum": 0},
            "width": {"type": "integer", "minimum": 1}, "height": {"type": "integer", "minimum": 1}})
        },
    },
];

/// What `tools/list` returns.
fn catalog() -> Vec<Value> {
    TOOLS.iter().map(Tool::describe).collect()
}

/// A JSON-RPC error response body.
fn rpc_error(code: i64, message: &str) -> Value {
    json!({"error": {"code": code, "message": message}})
}

/// A tool result: the JSON as text content, flagged as an error when the call failed.
fn tool_result(outcome: Result<Value, ApiError>) -> Value {
    let (text, is_error) = match outcome {
        Ok(body) => (body.to_string(), false),
        Err(ApiError::Internal(err)) => {
            tracing::error!(error = %err, "mcp internal error");
            ("internal server error".to_owned(), true)
        }
        Err(error) => (error.to_string(), true),
    };
    json!({"result": {"content": [{"type": "text", "text": text}], "isError": is_error}})
}

/// The reply body for one request, without the `jsonrpc` and `id` envelope.
async fn dispatch(state: Shared, principal: Principal, request: &Value) -> Value {
    let params = &request["params"];
    match request["method"].as_str() {
        Some("initialize") => {
            let version = params["protocolVersion"]
                .as_str()
                .filter(|version| PROTOCOL_VERSIONS.contains(version))
                .unwrap_or(DEFAULT_PROTOCOL_VERSION);
            json!({"result": {"protocolVersion": version, "capabilities": {"tools": {}},
                "serverInfo": {"name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION")}}})
        }
        Some("ping") => json!({"result": {}}),
        Some("tools/list") => json!({"result": {"tools": catalog()}}),
        Some("tools/call") => {
            let mut call = params.clone();
            if call.get("arguments").is_none() {
                call["arguments"] = json!({});
            }
            match serde_json::from_value::<Call>(call) {
                Ok(call) => tool_result(call.run(state, principal).await),
                Err(error) => rpc_error(INVALID_PARAMS, &error.to_string()),
            }
        }
        _ => rpc_error(METHOD_NOT_FOUND, "Method not found"),
    }
}

/// `POST /mcp`
pub async fn serve(
    State(state): State<Shared>,
    principal: Principal,
    body: axum::body::Bytes,
) -> Response {
    let (id, mut reply) = match serde_json::from_slice::<Value>(&body) {
        Err(_) => (Value::Null, rpc_error(PARSE_ERROR, "Parse error")),
        Ok(request) if request["jsonrpc"] != "2.0" || !request["method"].is_string() => {
            (Value::Null, rpc_error(INVALID_REQUEST, "Invalid request"))
        }
        // A notification has no id and gets no reply.
        Ok(request) if request.get("id").is_none() => return StatusCode::ACCEPTED.into_response(),
        Ok(request) if !(request["id"].is_number() || request["id"].is_string()) => {
            (Value::Null, rpc_error(INVALID_REQUEST, "Invalid request"))
        }
        Ok(request) => (
            request["id"].clone(),
            dispatch(state, principal, &request).await,
        ),
    };
    reply["jsonrpc"] = json!("2.0");
    reply["id"] = id;
    Json(reply).into_response()
}

#[cfg(test)]
mod tests {
    use super::{Call, TOOLS};
    use crate::testkit::*;
    use axum::http::StatusCode;
    use base64::Engine as _;
    use serde_json::{Value, json};

    /// Calls `/mcp` as `token` and returns the JSON-RPC reply.
    pub(super) async fn rpc(port: u16, token: &str, body: &Value) -> Value {
        let response = call(port, "POST", "/mcp", Some(token), Some(&body.to_string())).await;
        assert_eq!(status(&response), StatusCode::OK, "{response}");
        json(&response)
    }

    pub(super) fn tool(name: &str, arguments: &Value) -> Value {
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}})
    }

    /// The JSON a successful tool call returned.
    pub(super) fn payload(reply: &Value) -> Value {
        assert_eq!(reply["result"]["isError"], false, "{reply}");
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn every_catalog_entry_is_a_call_variant() {
        for tool in TOOLS {
            let outcome =
                serde_json::from_value::<Call>(json!({"name": tool.name, "arguments": {}}));
            if let Err(error) = outcome {
                assert!(
                    !error.to_string().contains("unknown variant"),
                    "{} has no Call variant: {error}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn catalog_matches_workspace_ops_exactly() {
        fn assert_server_scopes<
            T: rupicola_api::WorkspaceOps + rupicola_api::AuditorOps + rupicola_api::FullAgentOps,
        >() {
        }
        assert_server_scopes::<crate::http::ops::ServerOps>();
        let catalog_names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        assert_eq!(catalog_names, rupicola_api::WORKSPACE_OP_NAMES);
    }

    #[tokio::test]
    async fn extended_workspace_ops_work_through_mcp() {
        let (port, [ana, ..]) = demo_with_artifacts().await;
        let me = payload(&rpc(port, &ana, &tool("me", &json!({}))).await);
        assert_eq!(me["handle"], "ana");

        let limits = payload(
            &rpc(
                port,
                &ana,
                &tool("project_limits", &json!({"project": "demo"})),
            )
            .await,
        );
        assert!(limits["max_blob_bytes"].as_u64().unwrap() > 0);

        let topic = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "topic_create",
                    &json!({"project": "demo", "path": "auth/login"}),
                ),
            )
            .await,
        );
        assert_eq!(topic["name"], "login");
        let tree =
            payload(&rpc(port, &ana, &tool("topic_tree", &json!({"project": "demo"}))).await);
        assert_ne!(tree["topics"].as_array().unwrap().len(), 0);

        let ds = payload(
            &rpc(
                port,
                &ana,
                &tool("design_system_create", &json!({"project": "demo"})),
            )
            .await,
        );
        assert!(ds["id"].is_string());

        let rules = payload(&rpc(port, &ana, &tool("offline_rules", &json!({}))).await);
        assert_ne!(rules.as_array().unwrap().len(), 0);
        let modules = payload(&rpc(port, &ana, &tool("offline_modules", &json!({}))).await);
        assert_ne!(modules.as_array().unwrap().len(), 0);
    }

    pub(super) fn make_solid_png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, width, height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            let data: Vec<u8> = std::iter::repeat_n(rgba, (width * height) as usize)
                .flatten()
                .collect();
            w.write_image_data(&data).unwrap();
        }
        out
    }

    #[tokio::test]
    async fn visual_diff_crop_and_media_probe_work_through_mcp() {
        let (port, [ana, ..]) = demo_with_artifacts().await;
        let a_b64 = base64::engine::general_purpose::STANDARD.encode(make_solid_png(
            16,
            16,
            [20, 20, 20, 255],
        ));
        let b_b64 = base64::engine::general_purpose::STANDARD.encode(make_solid_png(
            16,
            16,
            [220, 40, 20, 255],
        ));

        let cap_a = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "capture_publish",
                    &json!({
                        "project": "demo",
                        "scenario": "diff-a",
                        "members": [{"id": "a", "data_base64": a_b64}]
                    }),
                ),
            )
            .await,
        );
        let cap_b = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "capture_publish",
                    &json!({
                        "project": "demo",
                        "scenario": "diff-b",
                        "members": [{"id": "b", "data_base64": b_b64}]
                    }),
                ),
            )
            .await,
        );
        let art_a = cap_a["checkpoints"][0]["artifact"]["id"].as_str().unwrap();
        let art_b = cap_b["checkpoints"][0]["artifact"]["id"].as_str().unwrap();

        let probe = payload(
            &rpc(
                port,
                &ana,
                &tool("media_probe", &json!({"artifact_id": art_a})),
            )
            .await,
        );
        assert_eq!(
            (probe["width"].as_u64(), probe["height"].as_u64()),
            (Some(16), Some(16))
        );

        let diff = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "visual_diff_images",
                    &json!({"before_artifact_id": art_a, "after_artifact_id": art_b, "mode": "highlight"}),
                ),
            )
            .await,
        );
        assert!(diff["changed_ratio"].as_f64().unwrap() > 0.99);
        assert_ne!(diff["changed_regions"].as_array().unwrap().len(), 0);
        let diff_id = diff["diff_artifact_id"].as_str().unwrap();
        let diff_probe = payload(
            &rpc(
                port,
                &ana,
                &tool("media_probe", &json!({"artifact_id": diff_id})),
            )
            .await,
        );
        assert_eq!(diff_probe["width"], 16);

        let crop = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "visual_crop_region",
                    &json!({"artifact_id": art_b, "x": 2, "y": 2, "width": 8, "height": 8}),
                ),
            )
            .await,
        );
        assert_eq!(crop["mime"], "image/png");
        assert!(crop["length"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn progressive_video_inspection_pipeline_end_to_end() {
        if std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let (port, [ana, ..]) = demo_with_artifacts().await;
        let dir = tempfile::tempdir().unwrap();
        let mp4_path = dir.path().join("flash.mp4");
        // 0.3s black + 0.1s white flash + 0.3s black at 30fps (21 frames total)
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:d=0.3:r=30",
                "-f",
                "lavfi",
                "-i",
                "color=c=white:s=64x64:d=0.1:r=30",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:d=0.3:r=30",
                "-filter_complex",
                "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
                "-map",
                "[v]",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&mp4_path)
            .status()
            .unwrap();
        assert!(status.success());

        let mp4_b64 =
            base64::engine::general_purpose::STANDARD.encode(std::fs::read(&mp4_path).unwrap());
        let cap = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "capture_publish",
                    &json!({
                        "project": "demo",
                        "scenario": "flash-clip",
                        "members": [{"id": "clip", "role": "clip-video", "mime": "video/mp4", "data_base64": mp4_b64}]
                    }),
                ),
            )
            .await,
        );
        let clip_art = cap["clips"][0]["video"]["id"].as_str().unwrap();

        // 1. Probe metadata (0 images in context)
        let probe = payload(
            &rpc(
                port,
                &ana,
                &tool("media_probe", &json!({"artifact_id": clip_art})),
            )
            .await,
        );
        assert_eq!(
            (probe["width"].as_u64(), probe["height"].as_u64()),
            (Some(64), Some(64))
        );
        assert!(probe["frame_count"].as_u64().unwrap() >= 15);

        // 2. Motion diff detects the white flash numerically in pure JSON
        let motion = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "media_motion_diff",
                    &json!({"artifact_id": clip_art, "threshold": 0.2}),
                ),
            )
            .await,
        );
        let spikes = motion["spikes"].as_array().unwrap();
        assert_ne!(spikes.len(), 0);
        assert!(
            spikes
                .iter()
                .any(|s| s["luma_delta"].as_f64().unwrap_or(0.0).abs() > 0.5),
            "expected strong luma flash spike in {motion}"
        );

        // 3. Contact sheet renders 1 PNG grid
        let sheet = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "media_contact_sheet",
                    &json!({"artifact_id": clip_art, "cols": 3, "rows": 2}),
                ),
            )
            .await,
        );
        assert_eq!(sheet["mime"], "image/png");
        assert!(sheet["length"].as_u64().unwrap() > 0);

        // 4. Progressive sampling: coarse then bisect excluding coarse frames
        let coarse = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "media_sample_frames",
                    &json!({"artifact_id": clip_art, "strategy": "coarse", "count": 5}),
                ),
            )
            .await,
        );
        let coarse_frames = coarse["frames"].as_array().unwrap();
        assert_eq!(coarse_frames.len(), 5);
        let coarse_indices: Vec<u64> = coarse_frames
            .iter()
            .map(|f| f["frame_index"].as_u64().unwrap())
            .collect();

        // Verify sampled frame artifact is readable via artifact_read
        let first_frame_art = coarse_frames[0]["artifact_id"].as_str().unwrap();
        let frame_chunk = payload(
            &rpc(
                port,
                &ana,
                &tool("artifact_read", &json!({"id": first_frame_art})),
            )
            .await,
        );
        assert_eq!(frame_chunk["mime"], "image/png");

        let bisect = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "media_sample_frames",
                    &json!({
                        "artifact_id": clip_art,
                        "strategy": "bisect",
                        "start_ms": 0,
                        "end_ms": 700,
                        "count": 5,
                        "exclude_frames": coarse_indices
                    }),
                ),
            )
            .await,
        );
        let bisect_frames = bisect["frames"].as_array().unwrap();
        assert_eq!(bisect_frames.len(), 5);
        for f in bisect_frames {
            let idx = f["frame_index"].as_u64().unwrap();
            assert!(
                !coarse_indices.contains(&idx),
                "bisect repeated frame {idx}"
            );
        }
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn full_workspace_lifecycle_through_mcp() {
        let (port, [ana, ..]) = demo_with_artifacts().await;

        // Project lifecycle & capture settings
        let proj = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "project_create",
                    &json!({"id": "int-proj", "label": "Int Initial"}),
                ),
            )
            .await,
        );
        assert_eq!(proj["id"], "int-proj");
        let renamed = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "project_update",
                    &json!({"id": "int-proj", "label": "Int Renamed"}),
                ),
            )
            .await,
        );
        assert_eq!(renamed["label"], "Int Renamed");
        let settings = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "capture_settings_update",
                    &json!({"project": "int-proj", "auto_recapture": true, "base_ref": "develop"}),
                ),
            )
            .await,
        );
        assert_eq!(settings["auto_recapture"], true);

        // Publish two captures, link lineage, compare
        let png_a = base64::engine::general_purpose::STANDARD.encode(make_solid_png(
            8,
            8,
            [10, 10, 10, 255],
        ));
        let png_b = base64::engine::general_purpose::STANDARD.encode(make_solid_png(
            8,
            8,
            [200, 10, 10, 255],
        ));
        let cap1 = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "capture_publish",
                    &json!({"project": "int-proj", "scenario": "s1", "members": [{"id": "home", "data_base64": png_a}]}),
                ),
            )
            .await,
        );
        let cap2 = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "capture_publish",
                    &json!({"project": "int-proj", "scenario": "s1", "members": [{"id": "home", "data_base64": png_b}]}),
                ),
            )
            .await,
        );
        let (c1, c2) = (cap1["id"].as_str().unwrap(), cap2["id"].as_str().unwrap());
        let a1 = cap1["checkpoints"][0]["artifact"]["id"].as_str().unwrap();

        let parents =
            payload(&rpc(port, &ana, &tool("capture_parents_get", &json!({"id": c2}))).await);
        let generation = parents["generation"].as_i64().unwrap();
        let edited = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "capture_parents_edit",
                    &json!({
                        "id": c2,
                        "generation": generation,
                        "reason": "link baseline",
                        "parents": [{"id": c1, "kind": "baseline"}]
                    }),
                ),
            )
            .await,
        );
        assert_eq!(edited["parents"].as_array().unwrap().len(), 1);
        let graph = payload(&rpc(port, &ana, &tool("capture_lineage", &json!({"id": c2}))).await);
        assert_eq!(graph["nodes"].as_array().unwrap().len(), 2);
        let cmp = payload(
            &rpc(
                port,
                &ana,
                &tool("capture_compare", &json!({"before": c1, "after": c2})),
            )
            .await,
        );
        assert_eq!(cmp["changed"].as_array().unwrap().len(), 1);

        // Topic filing + selection + problem + fix + citations
        let topic = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "topic_create",
                    &json!({"project": "int-proj", "path": "nav/header"}),
                ),
            )
            .await,
        );
        let tid = topic["id"].as_str().unwrap();
        let _ = payload(
            &rpc(
                port,
                &ana,
                &tool("topic_file_capture", &json!({"id": tid, "capture": c1})),
            )
            .await,
        );
        let tcaps = payload(&rpc(port, &ana, &tool("topic_captures", &json!({"id": tid}))).await);
        assert_eq!(tcaps.as_array().unwrap().len(), 1);

        let sel = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "selection_create",
                    &json!({
                        "project": "int-proj",
                        "label": "Header crop",
                        "items": [{"capture": c1, "artifact": a1, "mark": "proof"}]
                    }),
                ),
            )
            .await,
        );
        let sid = sel["id"].as_str().unwrap();

        let filed_problem = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "problem_create",
                    &json!({
                        "project": "int-proj",
                        "title": "Header contrast",
                        "severity": "major",
                        "origin": "agent-visual",
                        "evidence": [a1]
                    }),
                ),
            )
            .await,
        );
        let pid = filed_problem["id"].as_str().unwrap();
        let cited_prob = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "problem_cite_selection",
                    &json!({"id": pid, "selection": sid}),
                ),
            )
            .await,
        );
        assert_eq!(cited_prob["selections"].as_array().unwrap().len(), 2);

        let detailed_probs = payload(
            &rpc(
                port,
                &ana,
                &tool("problem_list_detailed", &json!({"project": "int-proj"})),
            )
            .await,
        );
        assert_eq!(detailed_probs.as_array().unwrap().len(), 1);

        let citations =
            payload(&rpc(port, &ana, &tool("capture_citations", &json!({"id": c1}))).await);
        assert_ne!(citations["citations"].as_array().unwrap().len(), 0);
        assert!(citations["confirm"].is_string());
    }

    #[tokio::test]
    async fn an_agent_files_a_problem_and_a_fix_through_mcp() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        let listed = rpc(
            port,
            &ana,
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        )
        .await;
        assert_eq!(
            listed["result"]["tools"].as_array().unwrap().len(),
            TOOLS.len()
        );

        let problem = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "problem_create",
                    &json!({"project": "demo", "title": "Cramped", "severity": "minor",
                        "origin": "agent-visual", "evidence": [PROJECT_ARTIFACT]}),
                ),
            )
            .await,
        );
        let problem_id = problem["id"].as_str().unwrap();
        let fix = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "fix_create",
                    &json!({"project": "demo", "title": "Space", "problems": [problem_id]}),
                ),
            )
            .await,
        );
        let approved = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "fix_review",
                    &json!({"id": fix["id"], "outcome": "approved"}),
                ),
            )
            .await,
        );
        assert_eq!(approved["status"], "approved");
        let resolved = payload(
            &rpc(
                port,
                &bruno,
                &tool("problem_get", &json!({"id": problem_id})),
            )
            .await,
        );
        assert_eq!(resolved["status"], "resolved");
    }

    #[tokio::test]
    async fn an_agent_queues_watches_and_cancels_a_capture_job_through_mcp() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "scenario_put",
                    &json!({"project": "demo", "scenario": {"name": "home", "url": "/"}}),
                ),
            )
            .await,
        );
        let job = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "job_create",
                    &json!({"project": "demo", "scenario": "home",
                        "trees": [{"name": "base", "base": "develop"}]}),
                ),
            )
            .await,
        );
        assert_eq!(
            (job["status"].as_str(), job["cells_total"].as_u64()),
            (Some("queued"), Some(1))
        );
        let open = payload(
            &rpc(
                port,
                &bruno,
                &tool("job_list", &json!({"project": "demo", "open": true})),
            )
            .await,
        );
        assert_eq!(open.as_array().unwrap().len(), 1);
        let cancelled =
            payload(&rpc(port, &ana, &tool("job_cancel", &json!({"id": job["id"]}))).await);
        assert_eq!(cancelled["status"], "cancelled");
        let fetched =
            payload(&rpc(port, &bruno, &tool("job_get", &json!({"id": job["id"]}))).await);
        assert_eq!(fetched["status"], "cancelled");
    }

    #[tokio::test]
    async fn an_agent_pushes_and_reads_a_scenario_through_mcp() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        let scenario = json!({"name": "home", "url": "/",
            "matrix": {"viewports": ["390x844@2"], "themes": ["dark", "light"]}});
        let put = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "scenario_put",
                    &json!({"project": "demo", "scenario": scenario}),
                ),
            )
            .await,
        );
        let hash = put["hash"].as_str().unwrap().to_owned();
        let listed = payload(
            &rpc(
                port,
                &bruno,
                &tool("scenario_list", &json!({"project": "demo"})),
            )
            .await,
        );
        assert_eq!(
            (listed[0]["name"].as_str(), listed[0]["cells"].as_u64()),
            (Some("home"), Some(2))
        );
        let exact = payload(
            &rpc(
                port,
                &bruno,
                &tool(
                    "scenario_get",
                    &json!({"project": "demo", "name": "home", "hash": hash}),
                ),
            )
            .await,
        );
        assert_eq!(exact["scenario"]["url"], "/");
        let latest = payload(
            &rpc(
                port,
                &bruno,
                &tool("scenario_get", &json!({"project": "demo", "name": "home"})),
            )
            .await,
        );
        assert_eq!(latest["hash"], exact["hash"]);
    }

    #[tokio::test]
    async fn an_agent_publishes_a_capture_and_files_a_problem_through_mcp() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let png_b64 =
            base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nfake-png");
        let published = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "capture_publish",
                    &json!({
                        "project": "demo",
                        "scenario": "mcp-shot",
                        "viewport": "1280x800",
                        "theme": "dark",
                        "members": [{"id": "home", "role": "checkpoint", "mime": "image/png", "data_base64": png_b64}]
                    }),
                ),
            )
            .await,
        );
        let art_id = published["checkpoints"][0]["artifact"]["id"]
            .as_str()
            .expect("published capture returns checkpoint artifact id");
        let prob = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "problem_create",
                    &json!({
                        "project": "demo",
                        "title": "Filed purely via MCP",
                        "severity": "minor",
                        "origin": "agent-visual",
                        "evidence": [art_id]
                    }),
                ),
            )
            .await,
        );
        assert_eq!(prob["title"], "Filed purely via MCP");
    }

    #[tokio::test]
    async fn tool_failures_are_results_and_bad_requests_are_rpc_errors() {
        let (port, [_, bruno, _]) = demo_with_artifacts().await;
        let denied = rpc(
            port,
            &bruno,
            &tool("problem_create", &json!({"project": "demo", "title": "x", "severity": "minor", "origin": "user-reported"})),
        )
        .await;
        assert_eq!(denied["result"]["isError"], true, "{denied}");
        let unknown = rpc(port, &bruno, &tool("nope", &json!({}))).await;
        assert!(unknown["error"]["code"].is_i64(), "{unknown}");
        let anonymous = call(port, "POST", "/mcp", None, Some("{}")).await;
        assert_eq!(status(&anonymous), StatusCode::UNAUTHORIZED, "{anonymous}");
    }

    /// Content uploaded in the artifact test, and the part of it read back.
    const CONTENT: &str = "hello-world";
    const READ_OFFSET: u64 = 6;
    const READ_LENGTH: u64 = 5;
    const READ_BACK: &str = "world";

    #[tokio::test]
    async fn artifacts_are_read_in_pages() {
        let (port, [ana, ..]) = demo_with_artifacts().await;
        let sha = stream_blob(port, &ana, "demo", CONTENT.as_bytes()).await;
        let record = capture_record("cap-mcp-read", &sha, CONTENT.len() as u64, vec![]);
        let capture = publish_record(port, &ana, &record).await;
        let capture: rupicola_api::Capture = json(&capture);
        let artifact = &capture.checkpoints[0].artifact.id;
        let read = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "artifact_read",
                    &json!({"id": artifact, "offset": READ_OFFSET, "length": READ_LENGTH}),
                ),
            )
            .await,
        );
        assert_eq!(read["length"], READ_LENGTH);
        assert_eq!(read["bytes"], CONTENT.len());
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(read["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(decoded, READ_BACK.as_bytes());
    }

    #[tokio::test]
    async fn capture_capabilities_and_selection_schemas_work() {
        const CAPTURE_SHOT: &str = "capture-shot";
        let (port, [ana, ..]) = demo_with_artifacts().await;
        let caps = payload(&rpc(port, &ana, &tool("capture_capabilities", &json!({}))).await);
        assert!(caps["backends"].is_array(), "{caps}");

        let selection = json!([{
            "label": "Proof",
            "items": [{"capture": CAPTURE_SHOT, "artifact": PROJECT_ARTIFACT, "mark": "proof"}]
        }]);
        let problem = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "problem_create",
                    &json!({"project": "demo", "title": "Alignment", "severity": "minor",
                        "origin": "agent-visual", "selections": selection}),
                ),
            )
            .await,
        );
        assert_eq!(problem["selections"].as_array().unwrap().len(), 1);

        let fix = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "fix_create",
                    &json!({"project": "demo", "title": "Aligned", "problems": [problem["id"]],
                        "baseline": selection, "proof": selection}),
                ),
            )
            .await,
        );
        assert_eq!(fix["selections"].as_array().unwrap().len(), 2);
    }
}

#[cfg(test)]
#[path = "mcp_p0_tests.rs"]
mod p0_tests;
