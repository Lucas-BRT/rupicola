use crate::{
    domain::{ApiError, Principal},
    http::ops::ServerOps,
    state::Shared,
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
pub(super) enum Call {
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
    CaptureRecapture {
        id: String,
        #[serde(flatten)]
        form: rupicola_api::RecaptureRequest,
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
    pub(super) async fn run(self, state: Shared, principal: Principal) -> Result<Value, ApiError> {
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
            Self::CaptureRecapture { id, form } => value(&ops.capture_recapture(&id, &form).await?),
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
