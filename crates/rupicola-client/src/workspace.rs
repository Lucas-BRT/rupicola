//! Token-bound client implementing all atomic `rupicola_api` operation traits
//! over any [`Api`] transport plus local `rupicola-core` offline analysis.
//!
//! Via blanket supertrait impls, `WorkspaceClient<A>` satisfies `WorkspaceOps`,
//! `AuditorOps`, `FullAgentOps`, `RunnerOps`, `AdminAuthOps`, `CliOps`, and `PanelOps`.

use crate::{Api, ClientError, McpReply, McpTransport, Result};
use async_trait::async_trait;
use base64::Engine as _;
use rupicola_api::{
    ArtifactBytes, ArtifactChunk, ArtifactReadOps, AuditEvent, AuthSessionOps, BlobTransferOps,
    Capture, CaptureCapabilities, CaptureCatalogOps, CaptureDiff, CaptureExecutionOps,
    CaptureLifecycleOps, CaptureLineageOps, CapturePage, CapturePublishOps, CaptureRemoval,
    CaptureSummary, Citations, Comment, Credential, Credentials, DesignEntry, DesignLogOps,
    DesignSystemReadOps, DesignSystemWriteOps, DocumentEdit, Finding, Fix, FixReadOps,
    FixSelectionLink, FixSelectionOps, FixSummary, FixWriteOps, IdentityOps, IssuedCredential,
    IssuedInvite, Job, JobFailure, JobProgress, JobQueueOps, JobResult, JobWorkerOps, Limits,
    Lineage, LineageGraph, Me, MediaInspectOps, Member, Missing, MissingQuery, NewComment,
    NewCredential, NewDesignEntry, NewFinding, NewFix, NewInvite, NewJob, NewProblem, NewProject,
    NewReview, NewSelection, NewStep, NewTopic, OfflineCatalogOps, OfflineScanOps, OpsBase,
    ParentsEdit, PasswordChange, Problem, ProblemPatch, ProblemReadOps, ProblemSelectionOps,
    ProblemSummary, ProblemWriteOps, Project, ProjectAdminOps, ProjectCrudOps, ProjectPolicyOps,
    ProjectUpdate, PublishCaptureRequest, RecaptureSettings, RecaptureSettingsPatch, Redeem,
    Register, Scenario, ScenarioOps, ScenarioSummary, ScenarioVersion, Selection, SelectionOps,
    SessionToken, Setup, Step, SystemActionForm, SystemList, SystemVersion, TokenOps, Topic,
    TopicMove, TopicReadOps, TopicRename, TopicTree, TopicWriteOps, VisualDiffOps,
    record::CaptureRecord,
};
use serde_json::{Value, json};
use std::path::Path;

/// A token-bound client that implements every atomic operation trait over any [`Api`]
/// transport plus local `rupicola-core` offline analysis.
#[derive(Clone, Debug)]
pub struct WorkspaceClient<A: Api> {
    api: A,
    token: String,
}

impl<A: Api> WorkspaceClient<A> {
    pub fn new(api: A, token: impl Into<String>) -> Self {
        Self {
            api,
            token: token.into(),
        }
    }

    pub const fn api(&self) -> &A {
        &self.api
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

impl<A: Api> OpsBase for WorkspaceClient<A> {
    type Error = ClientError;
}

fn parse_total_from_range(header: Option<&str>, fallback: usize) -> u64 {
    header
        .and_then(|v| v.rsplit_once('/'))
        .and_then(|(_, total)| total.parse::<u64>().ok())
        .unwrap_or_else(|| u64::try_from(fallback).unwrap_or(u64::MAX))
}

#[async_trait]
impl<A: Api> McpTransport for WorkspaceClient<A> {
    async fn send(&self, payload: &str) -> Result<McpReply> {
        self.api.mcp(&self.token, payload).await
    }
}

#[async_trait]
impl<A: Api> IdentityOps for WorkspaceClient<A> {
    async fn me(&self) -> Result<Me> {
        self.api.me(&self.token).await
    }
}

#[async_trait]
impl<A: Api> ProjectCrudOps for WorkspaceClient<A> {
    async fn project_list(&self) -> Result<Vec<Project>> {
        self.api.projects(&self.token).await
    }

    async fn project_get(&self, id: &str) -> Result<Project> {
        self.api.project(&self.token, id).await
    }

    async fn project_create(&self, form: &NewProject) -> Result<Project> {
        self.api.create_project(&self.token, form).await
    }

    async fn project_update(&self, id: &str, form: &ProjectUpdate) -> Result<Project> {
        self.api.update_project(&self.token, id, form).await
    }

    async fn project_delete(&self, id: &str) -> Result<()> {
        self.api.delete_project(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> ProjectPolicyOps for WorkspaceClient<A> {
    async fn project_limits(&self, project: &str) -> Result<Limits> {
        self.api.project_limits(&self.token, project).await
    }

    async fn capture_settings_get(&self, project: &str) -> Result<RecaptureSettings> {
        self.api.capture_settings(&self.token, project).await
    }

    async fn capture_settings_update(
        &self,
        project: &str,
        patch: &RecaptureSettingsPatch,
    ) -> Result<RecaptureSettings> {
        self.api
            .update_capture_settings(&self.token, project, patch)
            .await
    }
}

#[async_trait]
impl<A: Api> CaptureCatalogOps for WorkspaceClient<A> {
    async fn capture_list(&self, project: &str) -> Result<Vec<CaptureSummary>> {
        self.api.captures(&self.token, project).await
    }

    async fn capture_search(&self, project: &str, query: Option<&str>) -> Result<CapturePage> {
        self.api
            .search_captures(&self.token, project, query.map(str::to_owned))
            .await
    }

    async fn capture_get(&self, id: &str) -> Result<Capture> {
        self.api.capture(&self.token, id).await
    }

    async fn capture_capabilities(&self) -> Result<CaptureCapabilities> {
        self.api.capture_capabilities(&self.token).await
    }
}

#[async_trait]
impl<A: Api> CapturePublishOps for WorkspaceClient<A> {
    async fn capture_publish(&self, req: &PublishCaptureRequest) -> Result<Capture> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "capture_publish", "arguments": req }
        })
        .to_string();
        let reply = self.api.mcp(&self.token, &payload).await?;
        let parsed: Value =
            serde_json::from_str(&reply.body).map_err(|e| ClientError::Malformed(e.to_string()))?;
        if let Some(msg) = parsed["error"]["message"].as_str() {
            return Err(ClientError::Invalid(msg.to_owned()));
        }
        let text = parsed["result"]["content"][0]["text"]
            .as_str()
            .ok_or_else(|| ClientError::Malformed("missing MCP tool content".to_owned()))?;
        if parsed["result"]["isError"].as_bool() == Some(true) {
            return Err(ClientError::Invalid(text.to_owned()));
        }
        serde_json::from_str(text).map_err(|e| ClientError::Malformed(e.to_string()))
    }

    async fn capture_review(&self, id: &str, finding: &NewFinding) -> Result<Finding> {
        self.api.add_finding(&self.token, id, finding).await
    }
}

#[async_trait]
impl<A: Api> CaptureLifecycleOps for WorkspaceClient<A> {
    async fn capture_citations(&self, id: &str) -> Result<Citations> {
        self.api.capture_citations(&self.token, id).await
    }

    async fn capture_delete(&self, id: &str, form: &CaptureRemoval) -> Result<()> {
        self.api.delete_capture(&self.token, id, form).await
    }
}

#[async_trait]
impl<A: Api> CaptureLineageOps for WorkspaceClient<A> {
    async fn capture_lineage(&self, id: &str, depth: Option<u32>) -> Result<LineageGraph> {
        self.api.capture_lineage(&self.token, id, depth).await
    }

    async fn capture_parents_get(&self, id: &str) -> Result<Lineage> {
        self.api.capture_parents(&self.token, id).await
    }

    async fn capture_parents_edit(&self, id: &str, form: &ParentsEdit) -> Result<Lineage> {
        self.api.edit_capture_parents(&self.token, id, form).await
    }

    async fn capture_compare(&self, before: &str, after: &str) -> Result<CaptureDiff> {
        self.api.compare_captures(&self.token, before, after).await
    }
}

#[async_trait]
impl<A: Api> ArtifactReadOps for WorkspaceClient<A> {
    async fn artifact_read(&self, id: &str, offset: u64, length: usize) -> Result<ArtifactChunk> {
        let len_u64 = u64::try_from(length.max(1)).unwrap_or(u64::MAX);
        let end = offset.saturating_add(len_u64).saturating_sub(1);
        let body = self
            .api
            .artifact(&self.token, id, Some(format!("bytes={offset}-{end}")))
            .await?;
        let total = parse_total_from_range(body.content_range.as_deref(), body.bytes.len());
        let len = body.bytes.len();
        Ok(ArtifactChunk {
            id: id.to_owned(),
            mime: body
                .content_type
                .unwrap_or_else(|| "application/octet-stream".to_owned()),
            bytes: total,
            offset,
            length: len,
            data: base64::engine::general_purpose::STANDARD.encode(body.bytes),
        })
    }
}

impl<A: Api> WorkspaceClient<A> {
    async fn call_mcp<T: serde::de::DeserializeOwned>(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<T> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        })
        .to_string();
        let reply = self.api.mcp(&self.token, &payload).await?;
        let parsed: Value =
            serde_json::from_str(&reply.body).map_err(|e| ClientError::Malformed(e.to_string()))?;
        if let Some(msg) = parsed["error"]["message"].as_str() {
            return Err(ClientError::Invalid(msg.to_owned()));
        }
        let text = parsed["result"]["content"][0]["text"]
            .as_str()
            .ok_or_else(|| ClientError::Malformed("missing MCP tool content".to_owned()))?;
        if parsed["result"]["isError"].as_bool() == Some(true) {
            return Err(ClientError::Invalid(text.to_owned()));
        }
        serde_json::from_str(text).map_err(|e| ClientError::Malformed(e.to_string()))
    }
}

#[async_trait]
impl<A: Api> CaptureExecutionOps for WorkspaceClient<A> {
    async fn capture_run_scenario(&self, req: &rupicola_api::CaptureRunRequest) -> Result<Capture> {
        self.call_mcp("capture_run_scenario", json!(req)).await
    }

    async fn capture_record_clip(&self, req: &rupicola_api::RecordClipRequest) -> Result<Capture> {
        self.call_mcp("capture_record_clip", json!(req)).await
    }

    async fn capture_snapshot_checkpoint(
        &self,
        req: &rupicola_api::SnapshotCheckpointRequest,
    ) -> Result<Capture> {
        self.call_mcp("capture_snapshot_checkpoint", json!(req))
            .await
    }
}

#[async_trait]
impl<A: Api> MediaInspectOps for WorkspaceClient<A> {
    async fn media_probe(&self, artifact_id: &str) -> Result<rupicola_api::MediaProbe> {
        self.call_mcp("media_probe", json!({"artifact_id": artifact_id}))
            .await
    }

    async fn media_motion_diff(
        &self,
        artifact_id: &str,
        threshold: f64,
    ) -> Result<rupicola_api::MotionDiffReport> {
        self.call_mcp(
            "media_motion_diff",
            json!({"artifact_id": artifact_id, "threshold": threshold}),
        )
        .await
    }

    async fn media_contact_sheet(
        &self,
        artifact_id: &str,
        req: &rupicola_api::ContactSheetRequest,
    ) -> Result<ArtifactChunk> {
        let mut args = json!(req);
        args["artifact_id"] = json!(artifact_id);
        self.call_mcp("media_contact_sheet", args).await
    }

    async fn media_sample_frames(
        &self,
        artifact_id: &str,
        strategy: &rupicola_api::FrameSampleStrategy,
    ) -> Result<rupicola_api::SampledFrames> {
        let mut args = json!(strategy);
        args["artifact_id"] = json!(artifact_id);
        self.call_mcp("media_sample_frames", args).await
    }
}

#[async_trait]
impl<A: Api> VisualDiffOps for WorkspaceClient<A> {
    async fn visual_diff_images(
        &self,
        before_artifact_id: &str,
        after_artifact_id: &str,
        mode: rupicola_api::VisualDiffMode,
    ) -> Result<rupicola_api::VisualDiffReport> {
        self.call_mcp(
            "visual_diff_images",
            json!({
                "before_artifact_id": before_artifact_id,
                "after_artifact_id": after_artifact_id,
                "mode": mode
            }),
        )
        .await
    }

    async fn visual_crop_region(
        &self,
        artifact_id: &str,
        rect: &rupicola_api::CropRect,
    ) -> Result<ArtifactChunk> {
        let mut args = json!(rect);
        args["artifact_id"] = json!(artifact_id);
        self.call_mcp("visual_crop_region", args).await
    }
}

#[async_trait]
impl<A: Api> ProblemReadOps for WorkspaceClient<A> {
    async fn problem_list(&self, project: &str) -> Result<Vec<ProblemSummary>> {
        self.api.problems(&self.token, project).await
    }

    async fn problem_list_detailed(&self, project: &str) -> Result<Vec<Problem>> {
        self.api.problems_detailed(&self.token, project).await
    }

    async fn problem_get(&self, id: &str) -> Result<Problem> {
        self.api.problem(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> ProblemWriteOps for WorkspaceClient<A> {
    async fn problem_create(&self, project: &str, form: &NewProblem) -> Result<Problem> {
        self.api.create_problem(&self.token, project, form).await
    }

    async fn problem_delete(&self, id: &str) -> Result<()> {
        self.api.delete_problem(&self.token, id).await
    }

    async fn problem_triage(&self, id: &str, patch: &ProblemPatch) -> Result<Problem> {
        self.api.update_problem(&self.token, id, patch).await
    }

    async fn problem_comment(&self, id: &str, form: &NewComment) -> Result<Comment> {
        self.api.comment_problem(&self.token, id, form).await
    }

    async fn problem_analyze(&self, project: &str, capture: &str) -> Result<Vec<Problem>> {
        self.api
            .analyze_capture(&self.token, project, capture)
            .await
    }
}

#[async_trait]
impl<A: Api> ProblemSelectionOps for WorkspaceClient<A> {
    async fn problem_cite_selection(&self, id: &str, selection: &str) -> Result<Problem> {
        self.api
            .cite_problem_selection(&self.token, id, selection)
            .await
    }

    async fn problem_uncite_selection(&self, id: &str, selection: &str) -> Result<Problem> {
        self.api
            .uncite_problem_selection(&self.token, id, selection)
            .await
    }
}

#[async_trait]
impl<A: Api> FixReadOps for WorkspaceClient<A> {
    async fn fix_list(&self, project: &str) -> Result<Vec<FixSummary>> {
        self.api.fixes(&self.token, project).await
    }

    async fn fix_list_detailed(&self, project: &str) -> Result<Vec<Fix>> {
        self.api.fixes_detailed(&self.token, project).await
    }

    async fn fix_get(&self, id: &str) -> Result<Fix> {
        self.api.fix(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> FixWriteOps for WorkspaceClient<A> {
    async fn fix_create(&self, project: &str, form: &NewFix) -> Result<Fix> {
        self.api.create_fix(&self.token, project, form).await
    }

    async fn fix_step(&self, id: &str, form: &NewStep) -> Result<Step> {
        self.api.add_step(&self.token, id, form).await
    }

    async fn fix_review(&self, id: &str, form: &NewReview) -> Result<Fix> {
        self.api.review_fix(&self.token, id, form).await
    }
}

#[async_trait]
impl<A: Api> FixSelectionOps for WorkspaceClient<A> {
    async fn fix_cite_selection(
        &self,
        id: &str,
        selection: &str,
        link: &FixSelectionLink,
    ) -> Result<Fix> {
        self.api
            .cite_fix_selection(&self.token, id, selection, link)
            .await
    }

    async fn fix_uncite_selection(&self, id: &str, selection: &str) -> Result<Fix> {
        self.api
            .uncite_fix_selection(&self.token, id, selection)
            .await
    }
}

#[async_trait]
impl<A: Api> SelectionOps for WorkspaceClient<A> {
    async fn selection_create(&self, project: &str, form: &NewSelection) -> Result<Selection> {
        self.api.create_selection(&self.token, project, form).await
    }

    async fn selection_get(&self, id: &str) -> Result<Selection> {
        self.api.selection(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> DesignLogOps for WorkspaceClient<A> {
    async fn design_list(&self, project: &str) -> Result<Vec<DesignEntry>> {
        self.api.design(&self.token, project).await
    }

    async fn design_record(&self, project: &str, form: &NewDesignEntry) -> Result<DesignEntry> {
        self.api.add_design(&self.token, project, form).await
    }
}

#[async_trait]
impl<A: Api> DesignSystemReadOps for WorkspaceClient<A> {
    async fn design_system_list(&self, project: &str) -> Result<SystemList> {
        self.api.design_systems(&self.token, project).await
    }

    async fn design_system_get(&self, project: &str, version: &str) -> Result<SystemVersion> {
        self.api.design_system(&self.token, project, version).await
    }
}

#[async_trait]
impl<A: Api> DesignSystemWriteOps for WorkspaceClient<A> {
    async fn design_system_create(&self, project: &str) -> Result<SystemVersion> {
        self.api.create_design_system(&self.token, project).await
    }

    async fn design_system_edit(
        &self,
        project: &str,
        version: &str,
        edit: &DocumentEdit,
    ) -> Result<SystemVersion> {
        self.api
            .edit_design_system(&self.token, project, version, edit)
            .await
    }

    async fn design_system_act(
        &self,
        project: &str,
        version: &str,
        form: &SystemActionForm,
    ) -> Result<SystemVersion> {
        self.api
            .act_on_design_system(&self.token, project, version, form)
            .await
    }
}

#[async_trait]
impl<A: Api> JobQueueOps for WorkspaceClient<A> {
    async fn job_list(&self, project: &str, open: bool) -> Result<Vec<Job>> {
        self.api.jobs(&self.token, project, open).await
    }

    async fn job_get(&self, id: &str) -> Result<Job> {
        self.api.job(&self.token, id).await
    }

    async fn job_create(&self, project: &str, form: &NewJob) -> Result<Job> {
        self.api.create_job(&self.token, project, form).await
    }

    async fn job_cancel(&self, id: &str) -> Result<Job> {
        self.api.cancel_job(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> JobWorkerOps for WorkspaceClient<A> {
    async fn job_claim(&self, project: &str) -> Result<Option<Job>> {
        self.api.claim_job(&self.token, project).await
    }

    async fn job_progress(&self, id: &str, progress: &JobProgress) -> Result<Job> {
        self.api.job_progress(&self.token, id, progress).await
    }

    async fn job_complete(&self, id: &str, result: &JobResult) -> Result<Job> {
        self.api.complete_job(&self.token, id, result).await
    }

    async fn job_fail(&self, id: &str, failure: &JobFailure) -> Result<Job> {
        self.api.fail_job(&self.token, id, failure).await
    }
}

#[async_trait]
impl<A: Api> ScenarioOps for WorkspaceClient<A> {
    async fn scenario_list(&self, project: &str) -> Result<Vec<ScenarioSummary>> {
        self.api.scenarios(&self.token, project).await
    }

    async fn scenario_get(
        &self,
        project: &str,
        name: &str,
        hash: Option<&str>,
    ) -> Result<ScenarioVersion> {
        match hash {
            Some(h) => {
                self.api
                    .scenario_version(&self.token, project, name, h)
                    .await
            }
            None => self.api.scenario(&self.token, project, name).await,
        }
    }

    async fn scenario_put(&self, project: &str, scenario: &Scenario) -> Result<ScenarioVersion> {
        self.api.push_scenario(&self.token, project, scenario).await
    }
}

#[async_trait]
impl<A: Api> TopicReadOps for WorkspaceClient<A> {
    async fn topic_tree(&self, project: &str) -> Result<TopicTree> {
        self.api.topic_tree(&self.token, project).await
    }

    async fn topic_captures(&self, id: &str, direct: bool) -> Result<Vec<CaptureSummary>> {
        self.api.topic_captures(&self.token, id, direct).await
    }
}

#[async_trait]
impl<A: Api> TopicWriteOps for WorkspaceClient<A> {
    async fn topic_create(&self, project: &str, form: &NewTopic) -> Result<Topic> {
        self.api.create_topic(&self.token, project, form).await
    }

    async fn topic_rename(&self, id: &str, form: &TopicRename) -> Result<Topic> {
        self.api.rename_topic(&self.token, id, form).await
    }

    async fn topic_move(&self, id: &str, form: &TopicMove) -> Result<Topic> {
        self.api.move_topic(&self.token, id, form).await
    }

    async fn topic_delete(&self, id: &str, recursive: bool) -> Result<()> {
        self.api.delete_topic(&self.token, id, recursive).await
    }

    async fn topic_file_capture(&self, id: &str, capture: &str) -> Result<()> {
        self.api.file_topic_capture(&self.token, id, capture).await
    }

    async fn topic_unfile_capture(&self, id: &str, capture: &str) -> Result<()> {
        self.api
            .unfile_topic_capture(&self.token, id, capture)
            .await
    }
}

#[async_trait]
impl<A: Api> OfflineScanOps for WorkspaceClient<A> {
    async fn offline_discover(&self, path: &str) -> Result<Value> {
        let out = rupicola_core::discover::discover(Path::new(path))
            .map_err(|e| ClientError::Invalid(e.to_string()))?;
        serde_json::to_value(out).map_err(|e| ClientError::Malformed(e.to_string()))
    }

    async fn offline_scan(
        &self,
        path: &str,
        framework: Option<&str>,
        include_tests: bool,
    ) -> Result<Value> {
        let out = rupicola_core::scan::scan(Path::new(path), framework, include_tests)
            .map_err(|e| ClientError::Invalid(e.to_string()))?;
        serde_json::to_value(out).map_err(|e| ClientError::Malformed(e.to_string()))
    }
}

#[async_trait]
impl<A: Api> OfflineCatalogOps for WorkspaceClient<A> {
    async fn offline_check(&self, path: &str) -> Result<Value> {
        let out = rupicola_core::check::check(Path::new(path))
            .map_err(|e| ClientError::Invalid(e.to_string()))?;
        serde_json::to_value(out).map_err(|e| ClientError::Malformed(e.to_string()))
    }

    async fn offline_rules(&self) -> Result<Value> {
        serde_json::to_value(rupicola_core::scan::RULES)
            .map_err(|e| ClientError::Malformed(e.to_string()))
    }

    async fn offline_modules(&self) -> Result<Value> {
        let modules: Vec<String> = rupicola_core::FRAMEWORKS
            .iter()
            .map(|id| format!("reference/frameworks/{id}.md"))
            .chain(
                rupicola_core::TOPICS
                    .iter()
                    .map(|id| format!("reference/ux/{id}.md")),
            )
            .collect();
        serde_json::to_value(modules).map_err(|e| ClientError::Malformed(e.to_string()))
    }
}

#[async_trait]
impl<A: Api> BlobTransferOps for WorkspaceClient<A> {
    async fn missing_blobs(&self, project: &str, query: &MissingQuery) -> Result<Missing> {
        self.api.missing_blobs(&self.token, project, query).await
    }

    async fn upload_project_blob(&self, project: &str, sha256: &str, data: Vec<u8>) -> Result<()> {
        self.api
            .upload_project_blob(&self.token, project, sha256, data)
            .await
    }

    async fn publish_record(
        &self,
        project: &str,
        id: &str,
        record: &CaptureRecord,
    ) -> Result<Capture> {
        self.api
            .publish_record(&self.token, project, id, record)
            .await
    }

    async fn artifact_bytes(&self, id: &str, range: Option<String>) -> Result<ArtifactBytes> {
        let body = self.api.artifact(&self.token, id, range).await?;
        Ok(ArtifactBytes {
            status: body.status,
            content_type: body.content_type,
            content_range: body.content_range,
            bytes: body.bytes,
        })
    }
}

#[async_trait]
impl<A: Api> AuthSessionOps for WorkspaceClient<A> {
    async fn setup_required(&self) -> Result<bool> {
        self.api.setup_required().await
    }

    async fn setup(&self, form: &Setup) -> Result<SessionToken> {
        self.api.setup(form).await
    }

    async fn sign_in(&self, form: &Credentials) -> Result<SessionToken> {
        self.api.sign_in(form).await
    }

    async fn register(&self, form: &Register) -> Result<SessionToken> {
        self.api.register(form).await
    }

    async fn sign_out(&self) -> Result<()> {
        self.api.sign_out(&self.token).await
    }

    async fn sign_out_others(&self) -> Result<()> {
        self.api.sign_out_others(&self.token).await
    }

    async fn change_password(&self, form: &PasswordChange) -> Result<()> {
        self.api.change_password(&self.token, form).await
    }

    async fn redeem_invite(&self, form: &Redeem) -> Result<Project> {
        self.api.redeem_invite(&self.token, form).await
    }
}

#[async_trait]
impl<A: Api> TokenOps for WorkspaceClient<A> {
    async fn token_list(&self) -> Result<Vec<Credential>> {
        self.api.tokens(&self.token).await
    }

    async fn token_create(&self, form: &NewCredential) -> Result<IssuedCredential> {
        self.api.create_token(&self.token, form).await
    }

    async fn token_delete(&self, id: &str) -> Result<()> {
        self.api.delete_token(&self.token, id).await
    }
}

#[async_trait]
impl<A: Api> ProjectAdminOps for WorkspaceClient<A> {
    async fn member_list(&self, project: &str) -> Result<Vec<Member>> {
        self.api.members(&self.token, project).await
    }

    async fn member_remove(&self, project: &str, handle: &str) -> Result<()> {
        self.api.remove_member(&self.token, project, handle).await
    }

    async fn invite_create(&self, project: &str, form: &NewInvite) -> Result<IssuedInvite> {
        self.api.create_invite(&self.token, project, form).await
    }

    async fn audit_log(&self, project: &str) -> Result<Vec<AuditEvent>> {
        self.api.audit(&self.token, project).await
    }
}

#[cfg(test)]
mod tests {
    use super::WorkspaceClient;
    use crate::{ArtifactBody, ClientError, HttpApi, McpReply, McpTransport, MockApi};
    use base64::Engine as _;
    use rupicola_api::{
        AdminAuthOps, ArtifactReadOps, AuditorOps, CliOps, FullAgentOps, MediaInspectOps,
        OfflineCatalogOps, PanelOps, RunnerOps, WorkspaceOps,
    };

    #[test]
    fn workspace_client_satisfies_every_composed_supertrait() {
        fn assert_all<
            T: WorkspaceOps
                + AuditorOps
                + FullAgentOps
                + RunnerOps
                + AdminAuthOps
                + CliOps
                + PanelOps
                + McpTransport,
        >() {
        }
        assert_all::<WorkspaceClient<HttpApi>>();
    }

    #[tokio::test]
    async fn artifact_read_parses_content_range_and_encodes_base64() {
        let mut mock = MockApi::new();
        mock.expect_artifact()
            .times(1)
            .withf(|tok, id, range| {
                tok == "tok" && id == "art-1" && range.as_deref() == Some("bytes=6-10")
            })
            .returning(|_, _, _| {
                Ok(ArtifactBody {
                    status: 206,
                    content_type: Some("text/plain".to_owned()),
                    content_range: Some("bytes 6-10/11".to_owned()),
                    bytes: b"world".to_vec(),
                })
            });

        let client = WorkspaceClient::new(mock, "tok");
        let chunk = client.artifact_read("art-1", 6, 5).await.unwrap();
        assert_eq!(chunk.bytes, 11);
        assert_eq!(chunk.length, 5);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&chunk.data)
                .unwrap(),
            b"world"
        );
    }

    #[tokio::test]
    async fn mcp_tool_error_maps_to_client_invalid() {
        let mut mock = MockApi::new();
        mock.expect_mcp().times(1).returning(|_, _| {
            Ok(McpReply {
                status: 200,
                body: serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "content": [{"type": "text", "text": "ffprobe unavailable"}],
                        "isError": true
                    }
                })
                .to_string(),
            })
        });

        let client = WorkspaceClient::new(mock, "tok");
        let err = client.media_probe("art-9").await.unwrap_err();
        assert!(matches!(err, ClientError::Invalid(ref m) if m.contains("ffprobe")));
    }

    #[tokio::test]
    async fn offline_catalog_ops_run_locally_without_network() {
        let client = WorkspaceClient::new(MockApi::new(), "tok");
        let rules = client.offline_rules().await.unwrap();
        let modules = client.offline_modules().await.unwrap();
        assert_ne!(rules.as_array().unwrap().len(), 0);
        assert_ne!(modules.as_array().unwrap().len(), 0);
    }
}
