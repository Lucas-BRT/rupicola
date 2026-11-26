//! Server-side implementation of the atomic `rupicola_api` operation traits
//! backed directly by [`crate::services`] and [`rupicola_core`].
//!
//! Because supertraits (`WorkspaceOps`, `AuditorOps`, `FullAgentOps`) have blanket
//! implementations, `ServerOps` automatically satisfies them once the atomic
//! traits are implemented here.

use crate::{
    domain::{ApiError, Principal, secret::now_ms},
    http::handlers::search::parse_query,
    services::{
        access, capture_exec, captures, design, design_systems, fixes, jobs, limits, lineage,
        media, problems, projects, publish, removal, scenarios, search, selections, settings,
        topics, visual,
    },
    state::Shared,
};
use async_trait::async_trait;
use base64::Engine as _;
use rupicola_api::{
    ArtifactChunk, ArtifactReadOps, Capture, CaptureCapabilities, CaptureCatalogOps, CaptureDiff,
    CaptureExecutionOps, CaptureLifecycleOps, CaptureLineageOps, CapturePage, CapturePublishOps,
    CaptureRemoval, CaptureStatus, CaptureSummary, Citations, Comment, DesignEntry, DesignLogOps,
    DesignSystemReadOps, DesignSystemWriteOps, DocumentEdit, Finding, Fix, FixReadOps,
    FixSelectionLink, FixSelectionOps, FixSummary, FixWriteOps, IdentityOps, Job, JobFailure,
    JobProgress, JobQueueOps, JobResult, JobWorkerOps, Limits, Lineage, LineageGraph, Me,
    MediaInspectOps, NewComment, NewDesignEntry, NewFinding, NewFix, NewJob, NewProblem,
    NewProject, NewReview, NewSelection, NewStep, NewTopic, OfflineCatalogOps, OfflineScanOps,
    OpsBase, ParentsEdit, Problem, ProblemPatch, ProblemReadOps, ProblemSelectionOps,
    ProblemSummary, ProblemWriteOps, Project, ProjectCrudOps, ProjectPolicyOps, ProjectUpdate,
    PublishCaptureRequest, PublishMember, RecaptureSettings, RecaptureSettingsPatch, Role,
    Scenario, ScenarioOps, ScenarioSummary, ScenarioVersion, Selection, SelectionOps, Step,
    SystemActionForm, SystemList, SystemVersion, Topic, TopicMove, TopicReadOps, TopicRename,
    TopicTree, TopicWriteOps, VisualDiffOps,
    record::{
        Appearance, CaptureRecord, Display, Environment as RecordEnvironment, Host, Member,
        MemberRole, Platform, PlatformFamily, RECORD_FORMAT, RecordKind, ScenarioRef, Software,
    },
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write as _, path::Path};
use uuid::Uuid;

/// Most bytes one `artifact_read` may return.
pub(crate) const MAX_READ_BYTES: usize = 1024 * 1024;

/// Executes every atomic operation trait in-process against the server's services.
pub(crate) struct ServerOps {
    pub(crate) state: Shared,
    pub(crate) principal: Principal,
}

impl ServerOps {
    pub(crate) const fn new(state: Shared, principal: Principal) -> Self {
        Self { state, principal }
    }
}

impl OpsBase for ServerOps {
    type Error = ApiError;
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        })
}

fn parse_viewport(raw: &str) -> Option<(u32, u32)> {
    let clean = raw.trim().split('@').next()?;
    let (w, h) = clean.split_once(['x', 'X', '×'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn parse_member_role(raw: Option<&str>) -> Result<MemberRole, ApiError> {
    match raw
        .unwrap_or("checkpoint")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "checkpoint" => Ok(MemberRole::Checkpoint),
        "clip" | "clip-video" => Ok(MemberRole::ClipVideo),
        "clip-frame" => Ok(MemberRole::ClipFrame),
        "clip-sheet" => Ok(MemberRole::ClipSheet),
        "clip-analysis" => Ok(MemberRole::ClipAnalysis),
        "ui-snapshot" => Ok(MemberRole::UiSnapshot),
        other => Err(ApiError::invalid(format!(
            "unsupported member role `{other}`; expected checkpoint, clip-video, clip-frame, clip-sheet, clip-analysis or ui-snapshot"
        ))),
    }
}

fn infer_mime(role: MemberRole, path: &str) -> &'static str {
    match path
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("json") => "application/json",
        Some("png") => "image/png",
        _ => match role {
            MemberRole::ClipVideo => "video/mp4",
            MemberRole::ClipAnalysis | MemberRole::UiSnapshot => "application/json",
            MemberRole::Checkpoint | MemberRole::ClipFrame | MemberRole::ClipSheet => "image/png",
        },
    }
}

async fn build_publish_member(
    state: &Shared,
    project: &str,
    max_bytes: usize,
    idx: usize,
    item: &PublishMember,
) -> Result<Member, ApiError> {
    let role = parse_member_role(item.role.as_deref())?;
    let owner = item
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| format!("item-{}", idx + 1), str::to_owned);
    let default_ext = match role {
        MemberRole::ClipVideo => "mp4",
        MemberRole::ClipAnalysis | MemberRole::UiSnapshot => "json",
        MemberRole::Checkpoint | MemberRole::ClipFrame | MemberRole::ClipSheet => "png",
    };
    let default_dir = match role {
        MemberRole::Checkpoint | MemberRole::UiSnapshot => "checkpoints",
        MemberRole::ClipVideo
        | MemberRole::ClipFrame
        | MemberRole::ClipSheet
        | MemberRole::ClipAnalysis => "clips",
    };
    let path = item
        .path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(
            || format!("{default_dir}/{owner}.{default_ext}"),
            str::to_owned,
        );
    let mime = item
        .mime
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| infer_mime(role, &path).to_owned(), str::to_owned);

    let (sha256, bytes) = if let Some(b64) = &item.data_base64 {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| ApiError::invalid(format!("invalid base64 for `{path}`: {e}")))?;
        if decoded.len() > max_bytes {
            return Err(ApiError::invalid(format!(
                "`{path}` exceeds max artifact size ({max_bytes} bytes)"
            )));
        }
        let sha = sha256_hex(&decoded);
        let len = u64::try_from(decoded.len()).map_err(anyhow::Error::from)?;
        captures::put_project_blob(state, project, sha.clone(), decoded).await?;
        (sha, len)
    } else if let (Some(sha), Some(len)) = (&item.sha256, item.bytes) {
        (sha.clone(), len)
    } else {
        return Err(ApiError::invalid(format!(
            "member `{path}` requires `data_base64` (or `file_path` via rupicola mcp-bridge)"
        )));
    };

    Ok(Member {
        path,
        role,
        mime,
        sha256,
        bytes,
        owner,
        target_ms: None,
        video_ms: None,
        surface: None,
        locator: BTreeMap::new(),
    })
}

#[async_trait]
impl IdentityOps for ServerOps {
    async fn me(&self) -> Result<Me, ApiError> {
        Ok(Me {
            kind: self.principal.kind,
            handle: self.principal.handle.clone(),
            admin: self.principal.admin,
            project: self.principal.project.clone(),
        })
    }
}

#[async_trait]
impl ProjectCrudOps for ServerOps {
    async fn project_list(&self) -> Result<Vec<Project>, ApiError> {
        projects::list(&self.state, &self.principal).await
    }

    async fn project_get(&self, id: &str) -> Result<Project, ApiError> {
        projects::get(&self.state, &self.principal, id).await
    }

    async fn project_create(&self, form: &NewProject) -> Result<Project, ApiError> {
        projects::create(&self.state, &self.principal, form.clone()).await
    }

    async fn project_update(&self, id: &str, form: &ProjectUpdate) -> Result<Project, ApiError> {
        projects::rename(&self.state, &self.principal, id, &form.label).await
    }

    async fn project_delete(&self, id: &str) -> Result<(), ApiError> {
        projects::delete(&self.state, &self.principal, id).await
    }
}

#[async_trait]
impl ProjectPolicyOps for ServerOps {
    async fn project_limits(&self, project: &str) -> Result<Limits, ApiError> {
        limits::of_project(&self.state, &self.principal, project).await
    }

    async fn capture_settings_get(&self, project: &str) -> Result<RecaptureSettings, ApiError> {
        settings::capture_project(&self.state, &self.principal, project).await
    }

    async fn capture_settings_update(
        &self,
        project: &str,
        patch: &RecaptureSettingsPatch,
    ) -> Result<RecaptureSettings, ApiError> {
        settings::update_capture_project(&self.state, &self.principal, project, patch).await
    }
}

#[async_trait]
impl CaptureCatalogOps for ServerOps {
    async fn capture_list(&self, project: &str) -> Result<Vec<CaptureSummary>, ApiError> {
        captures::list(&self.state, &self.principal, project).await
    }

    async fn capture_search(
        &self,
        project: &str,
        query: Option<&str>,
    ) -> Result<CapturePage, ApiError> {
        let parsed = parse_query(query)?;
        search::find(&self.state, &self.principal, project, &parsed).await
    }

    async fn capture_get(&self, id: &str) -> Result<Capture, ApiError> {
        captures::get(&self.state, &self.principal, id).await
    }

    async fn capture_capabilities(&self) -> Result<CaptureCapabilities, ApiError> {
        Ok(captures::capabilities().await)
    }
}

#[async_trait]
impl CapturePublishOps for ServerOps {
    async fn capture_publish(&self, req: &PublishCaptureRequest) -> Result<Capture, ApiError> {
        access::require_publisher(&self.state, &self.principal, &req.project, Role::Reviewer)
            .await?;
        let scenario = req.scenario.trim();
        if scenario.is_empty() {
            return Err(ApiError::invalid("scenario must not be empty"));
        }
        if req.members.is_empty() {
            return Err(ApiError::invalid(
                "capture_publish requires at least one member",
            ));
        }
        let project_limits = limits::effective(&self.state, &req.project).await?;
        let max_bytes = usize::try_from(project_limits.max_blob_bytes).unwrap_or(usize::MAX);
        let mut members = Vec::with_capacity(req.members.len());
        for (idx, item) in req.members.iter().enumerate() {
            members
                .push(build_publish_member(&self.state, &req.project, max_bytes, idx, item).await?);
        }

        let display = req
            .viewport
            .as_deref()
            .and_then(parse_viewport)
            .map(|(w, h)| Display {
                width: Some(w),
                height: Some(h),
                scale_milli: Some(1000),
                orientation: None,
            });
        let vp_tag = display
            .as_ref()
            .and_then(|d| Some(format!("{}x{}", d.width?, d.height?)))
            .unwrap_or_else(|| "default".to_owned());
        let theme_tag = req.theme.as_deref().unwrap_or("default");
        let id = Uuid::now_v7().to_string();
        let record = CaptureRecord {
            format: RECORD_FORMAT,
            id: id.clone(),
            project: req.project.clone(),
            kind: RecordKind::Scenario,
            scenario: ScenarioRef {
                name: scenario.to_owned(),
                sha256: None,
            },
            revision: req.revision.clone().filter(|r| !r.trim().is_empty()),
            series: format!("{scenario}@{vp_tag}@{theme_tag}"),
            parents: Vec::new(),
            captured_at: now_ms(),
            status: CaptureStatus::Complete,
            blocker: None,
            environment: RecordEnvironment {
                platform: Some(Platform {
                    family: Some(PlatformFamily::Web),
                    os_version: None,
                }),
                display,
                appearance: req.theme.clone().map(|scheme| Appearance {
                    color_scheme: Some(scheme),
                    reduced_motion: None,
                    locale: None,
                    font_scale_milli: None,
                    contrast: None,
                }),
                software: Some(Software {
                    backend: Some("mcp-publish".to_owned()),
                    renderer: None,
                    app_version: None,
                    app_build: None,
                }),
                host: Some(Host {
                    os: Some(std::env::consts::OS.to_owned()),
                }),
                device: None,
                extra: BTreeMap::new(),
            },
            members,
        };

        let published =
            publish::publish(&self.state, &self.principal, &req.project, &id, &record).await?;
        Ok(match published {
            publish::Published::New(cap) | publish::Published::Same(cap) => cap,
        })
    }

    async fn capture_review(&self, id: &str, finding: &NewFinding) -> Result<Finding, ApiError> {
        captures::add_finding(&self.state, &self.principal, id, finding.clone()).await
    }
}

#[async_trait]
impl CaptureLifecycleOps for ServerOps {
    async fn capture_citations(&self, id: &str) -> Result<Citations, ApiError> {
        removal::citations(&self.state, &self.principal, id).await
    }

    async fn capture_delete(&self, id: &str, form: &CaptureRemoval) -> Result<(), ApiError> {
        removal::remove(&self.state, &self.principal, id, form).await
    }
}

#[async_trait]
impl CaptureLineageOps for ServerOps {
    async fn capture_lineage(
        &self,
        id: &str,
        depth: Option<u32>,
    ) -> Result<LineageGraph, ApiError> {
        lineage::graph(&self.state, &self.principal, id, depth).await
    }

    async fn capture_parents_get(&self, id: &str) -> Result<Lineage, ApiError> {
        lineage::get(&self.state, &self.principal, id).await
    }

    async fn capture_parents_edit(
        &self,
        id: &str,
        form: &ParentsEdit,
    ) -> Result<Lineage, ApiError> {
        lineage::edit(&self.state, &self.principal, id, form).await
    }

    async fn capture_compare(&self, before: &str, after: &str) -> Result<CaptureDiff, ApiError> {
        lineage::compare(&self.state, &self.principal, before, after).await
    }
}

#[async_trait]
impl ArtifactReadOps for ServerOps {
    async fn artifact_read(
        &self,
        id: &str,
        offset: u64,
        length: usize,
    ) -> Result<ArtifactChunk, ApiError> {
        let clamped = length.clamp(1, MAX_READ_BYTES);
        let (blob, data) =
            captures::read_artifact_range(&self.state, &self.principal, id, offset, clamped)
                .await?;
        Ok(ArtifactChunk {
            id: id.to_owned(),
            mime: blob.mime,
            bytes: u64::try_from(blob.bytes).unwrap_or(0),
            offset,
            length: data.len(),
            data: base64::engine::general_purpose::STANDARD.encode(data),
        })
    }
}

#[async_trait]
impl CaptureExecutionOps for ServerOps {
    async fn capture_run_scenario(
        &self,
        req: &rupicola_api::CaptureRunRequest,
    ) -> Result<Capture, ApiError> {
        capture_exec::run_scenario(&self.state, &self.principal, req).await
    }

    async fn capture_record_clip(
        &self,
        req: &rupicola_api::RecordClipRequest,
    ) -> Result<Capture, ApiError> {
        capture_exec::record_clip(&self.state, &self.principal, req).await
    }

    async fn capture_snapshot_checkpoint(
        &self,
        req: &rupicola_api::SnapshotCheckpointRequest,
    ) -> Result<Capture, ApiError> {
        capture_exec::snapshot_checkpoint(&self.state, &self.principal, req).await
    }
}

#[async_trait]
impl MediaInspectOps for ServerOps {
    async fn media_probe(&self, artifact_id: &str) -> Result<rupicola_api::MediaProbe, ApiError> {
        media::probe(&self.state, &self.principal, artifact_id).await
    }

    async fn media_motion_diff(
        &self,
        artifact_id: &str,
        threshold: f64,
    ) -> Result<rupicola_api::MotionDiffReport, ApiError> {
        media::motion_diff(&self.state, &self.principal, artifact_id, threshold).await
    }

    async fn media_contact_sheet(
        &self,
        artifact_id: &str,
        req: &rupicola_api::ContactSheetRequest,
    ) -> Result<ArtifactChunk, ApiError> {
        media::contact_sheet(&self.state, &self.principal, artifact_id, req).await
    }

    async fn media_sample_frames(
        &self,
        artifact_id: &str,
        strategy: &rupicola_api::FrameSampleStrategy,
    ) -> Result<rupicola_api::SampledFrames, ApiError> {
        media::sample_frames(&self.state, &self.principal, artifact_id, strategy).await
    }
}

#[async_trait]
impl VisualDiffOps for ServerOps {
    async fn visual_diff_images(
        &self,
        before_artifact_id: &str,
        after_artifact_id: &str,
        mode: rupicola_api::VisualDiffMode,
    ) -> Result<rupicola_api::VisualDiffReport, ApiError> {
        visual::diff_images(
            &self.state,
            &self.principal,
            before_artifact_id,
            after_artifact_id,
            mode,
        )
        .await
    }

    async fn visual_crop_region(
        &self,
        artifact_id: &str,
        rect: &rupicola_api::CropRect,
    ) -> Result<ArtifactChunk, ApiError> {
        visual::crop_region(&self.state, &self.principal, artifact_id, rect).await
    }
}

#[async_trait]
impl ProblemReadOps for ServerOps {
    async fn problem_list(&self, project: &str) -> Result<Vec<ProblemSummary>, ApiError> {
        problems::list(&self.state, &self.principal, project).await
    }

    async fn problem_list_detailed(&self, project: &str) -> Result<Vec<Problem>, ApiError> {
        problems::list_details(&self.state, &self.principal, project).await
    }

    async fn problem_get(&self, id: &str) -> Result<Problem, ApiError> {
        problems::get(&self.state, &self.principal, id).await
    }
}

#[async_trait]
impl ProblemWriteOps for ServerOps {
    async fn problem_create(&self, project: &str, form: &NewProblem) -> Result<Problem, ApiError> {
        problems::create(&self.state, &self.principal, project, form).await
    }

    async fn problem_delete(&self, id: &str) -> Result<(), ApiError> {
        problems::delete(&self.state, &self.principal, id).await
    }

    async fn problem_triage(&self, id: &str, patch: &ProblemPatch) -> Result<Problem, ApiError> {
        problems::update(&self.state, &self.principal, id, patch).await
    }

    async fn problem_comment(&self, id: &str, form: &NewComment) -> Result<Comment, ApiError> {
        problems::comment(&self.state, &self.principal, id, form).await
    }

    async fn problem_analyze(
        &self,
        project: &str,
        capture: &str,
    ) -> Result<Vec<Problem>, ApiError> {
        problems::analyze_capture(&self.state, &self.principal, project, capture).await
    }
}

#[async_trait]
impl ProblemSelectionOps for ServerOps {
    async fn problem_cite_selection(&self, id: &str, selection: &str) -> Result<Problem, ApiError> {
        problems::cite_selection(&self.state, &self.principal, id, selection).await
    }

    async fn problem_uncite_selection(
        &self,
        id: &str,
        selection: &str,
    ) -> Result<Problem, ApiError> {
        problems::uncite_selection(&self.state, &self.principal, id, selection).await
    }
}

#[async_trait]
impl FixReadOps for ServerOps {
    async fn fix_list(&self, project: &str) -> Result<Vec<FixSummary>, ApiError> {
        fixes::list(&self.state, &self.principal, project).await
    }

    async fn fix_list_detailed(&self, project: &str) -> Result<Vec<Fix>, ApiError> {
        fixes::list_details(&self.state, &self.principal, project).await
    }

    async fn fix_get(&self, id: &str) -> Result<Fix, ApiError> {
        fixes::get(&self.state, &self.principal, id).await
    }
}

#[async_trait]
impl FixWriteOps for ServerOps {
    async fn fix_create(&self, project: &str, form: &NewFix) -> Result<Fix, ApiError> {
        fixes::create(&self.state, &self.principal, project, form).await
    }

    async fn fix_step(&self, id: &str, form: &NewStep) -> Result<Step, ApiError> {
        fixes::add_step(&self.state, &self.principal, id, form.clone()).await
    }

    async fn fix_review(&self, id: &str, form: &NewReview) -> Result<Fix, ApiError> {
        fixes::review(&self.state, &self.principal, id, form).await
    }
}

#[async_trait]
impl FixSelectionOps for ServerOps {
    async fn fix_cite_selection(
        &self,
        id: &str,
        selection: &str,
        link: &FixSelectionLink,
    ) -> Result<Fix, ApiError> {
        fixes::cite_selection(&self.state, &self.principal, id, selection, link).await
    }

    async fn fix_uncite_selection(&self, id: &str, selection: &str) -> Result<Fix, ApiError> {
        fixes::uncite_selection(&self.state, &self.principal, id, selection).await
    }
}

#[async_trait]
impl SelectionOps for ServerOps {
    async fn selection_create(
        &self,
        project: &str,
        form: &NewSelection,
    ) -> Result<Selection, ApiError> {
        selections::create(&self.state, &self.principal, project, form).await
    }

    async fn selection_get(&self, id: &str) -> Result<Selection, ApiError> {
        selections::get(&self.state, &self.principal, id).await
    }
}

#[async_trait]
impl DesignLogOps for ServerOps {
    async fn design_list(&self, project: &str) -> Result<Vec<DesignEntry>, ApiError> {
        design::list(&self.state, &self.principal, project).await
    }

    async fn design_record(
        &self,
        project: &str,
        form: &NewDesignEntry,
    ) -> Result<DesignEntry, ApiError> {
        design::create(&self.state, &self.principal, project, form.clone()).await
    }
}

#[async_trait]
impl DesignSystemReadOps for ServerOps {
    async fn design_system_list(&self, project: &str) -> Result<SystemList, ApiError> {
        design_systems::list(&self.state, &self.principal, project).await
    }

    async fn design_system_get(
        &self,
        project: &str,
        version: &str,
    ) -> Result<SystemVersion, ApiError> {
        design_systems::get(&self.state, &self.principal, project, version).await
    }
}

#[async_trait]
impl DesignSystemWriteOps for ServerOps {
    async fn design_system_create(&self, project: &str) -> Result<SystemVersion, ApiError> {
        design_systems::create(&self.state, &self.principal, project).await
    }

    async fn design_system_edit(
        &self,
        project: &str,
        version: &str,
        edit: &DocumentEdit,
    ) -> Result<SystemVersion, ApiError> {
        design_systems::edit(&self.state, &self.principal, project, version, edit.clone()).await
    }

    async fn design_system_act(
        &self,
        project: &str,
        version: &str,
        form: &SystemActionForm,
    ) -> Result<SystemVersion, ApiError> {
        design_systems::act(&self.state, &self.principal, project, version, form).await
    }
}

#[async_trait]
impl JobQueueOps for ServerOps {
    async fn job_list(&self, project: &str, open: bool) -> Result<Vec<Job>, ApiError> {
        jobs::list(&self.state, &self.principal, project, open).await
    }

    async fn job_get(&self, id: &str) -> Result<Job, ApiError> {
        jobs::get(&self.state, &self.principal, id).await
    }

    async fn job_create(&self, project: &str, form: &NewJob) -> Result<Job, ApiError> {
        jobs::create(&self.state, &self.principal, project, form).await
    }

    async fn job_cancel(&self, id: &str) -> Result<Job, ApiError> {
        jobs::cancel(&self.state, &self.principal, id).await
    }
}

#[async_trait]
impl JobWorkerOps for ServerOps {
    async fn job_claim(&self, project: &str) -> Result<Option<Job>, ApiError> {
        jobs::claim(&self.state, &self.principal, project).await
    }

    async fn job_progress(&self, id: &str, progress: &JobProgress) -> Result<Job, ApiError> {
        jobs::progress(&self.state, &self.principal, id, *progress).await
    }

    async fn job_complete(&self, id: &str, result: &JobResult) -> Result<Job, ApiError> {
        jobs::complete(&self.state, &self.principal, id, result).await
    }

    async fn job_fail(&self, id: &str, failure: &JobFailure) -> Result<Job, ApiError> {
        jobs::fail(&self.state, &self.principal, id, failure).await
    }
}

#[async_trait]
impl ScenarioOps for ServerOps {
    async fn scenario_list(&self, project: &str) -> Result<Vec<ScenarioSummary>, ApiError> {
        scenarios::list(&self.state, &self.principal, project).await
    }

    async fn scenario_get(
        &self,
        project: &str,
        name: &str,
        hash: Option<&str>,
    ) -> Result<ScenarioVersion, ApiError> {
        match hash {
            Some(hash) => {
                scenarios::version(&self.state, &self.principal, project, name, hash).await
            }
            None => scenarios::latest(&self.state, &self.principal, project, name).await,
        }
    }

    async fn scenario_put(
        &self,
        project: &str,
        scenario: &Scenario,
    ) -> Result<ScenarioVersion, ApiError> {
        scenarios::put(&self.state, &self.principal, project, scenario)
            .await
            .map(|(version, _)| version)
    }
}

#[async_trait]
impl TopicReadOps for ServerOps {
    async fn topic_tree(&self, project: &str) -> Result<TopicTree, ApiError> {
        topics::tree(&self.state, &self.principal, project).await
    }

    async fn topic_captures(
        &self,
        id: &str,
        direct: bool,
    ) -> Result<Vec<CaptureSummary>, ApiError> {
        topics::captures(&self.state, &self.principal, id, direct).await
    }
}

#[async_trait]
impl TopicWriteOps for ServerOps {
    async fn topic_create(&self, project: &str, form: &NewTopic) -> Result<Topic, ApiError> {
        topics::create(&self.state, &self.principal, project, form)
            .await
            .map(|(topic, _)| topic)
    }

    async fn topic_rename(&self, id: &str, form: &TopicRename) -> Result<Topic, ApiError> {
        topics::rename(
            &self.state,
            &self.principal,
            id,
            &form.name,
            form.generation,
        )
        .await
    }

    async fn topic_move(&self, id: &str, form: &TopicMove) -> Result<Topic, ApiError> {
        topics::move_to(&self.state, &self.principal, id, form).await
    }

    async fn topic_delete(&self, id: &str, recursive: bool) -> Result<(), ApiError> {
        topics::remove(&self.state, &self.principal, id, recursive).await
    }

    async fn topic_file_capture(&self, id: &str, capture: &str) -> Result<(), ApiError> {
        topics::file(&self.state, &self.principal, id, capture).await
    }

    async fn topic_unfile_capture(&self, id: &str, capture: &str) -> Result<(), ApiError> {
        topics::unfile(&self.state, &self.principal, id, capture).await
    }
}

#[async_trait]
impl OfflineScanOps for ServerOps {
    async fn offline_discover(&self, path: &str) -> Result<Value, ApiError> {
        let out = rupicola_core::discover::discover(Path::new(path))
            .map_err(|e| ApiError::invalid(e.to_string()))?;
        Ok(serde_json::to_value(out).map_err(anyhow::Error::from)?)
    }

    async fn offline_scan(
        &self,
        path: &str,
        framework: Option<&str>,
        include_tests: bool,
    ) -> Result<Value, ApiError> {
        let out = rupicola_core::scan::scan(Path::new(path), framework, include_tests)
            .map_err(|e| ApiError::invalid(e.to_string()))?;
        Ok(serde_json::to_value(out).map_err(anyhow::Error::from)?)
    }
}

#[async_trait]
impl OfflineCatalogOps for ServerOps {
    async fn offline_check(&self, path: &str) -> Result<Value, ApiError> {
        let out = rupicola_core::check::check(Path::new(path))
            .map_err(|e| ApiError::invalid(e.to_string()))?;
        Ok(serde_json::to_value(out).map_err(anyhow::Error::from)?)
    }

    async fn offline_rules(&self) -> Result<Value, ApiError> {
        Ok(serde_json::to_value(rupicola_core::scan::RULES).map_err(anyhow::Error::from)?)
    }

    async fn offline_modules(&self) -> Result<Value, ApiError> {
        let modules: Vec<String> = rupicola_core::FRAMEWORKS
            .iter()
            .map(|id| format!("reference/frameworks/{id}.md"))
            .chain(
                rupicola_core::TOPICS
                    .iter()
                    .map(|id| format!("reference/ux/{id}.md")),
            )
            .collect();
        Ok(serde_json::to_value(modules).map_err(anyhow::Error::from)?)
    }
}
