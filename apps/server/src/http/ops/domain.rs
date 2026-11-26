use super::{MAX_READ_BYTES, ServerOps};
use crate::{
    domain::{ApiError, secret::now_ms},
    http::handlers::search::parse_query,
    services::{access, captures, limits, lineage, projects, publish, removal, search, settings},
    state::Shared,
};
use async_trait::async_trait;
use base64::Engine as _;
use rupicola_api::{
    ArtifactChunk, ArtifactReadOps, Capture, CaptureCapabilities, CaptureCatalogOps, CaptureDiff,
    CaptureLifecycleOps, CaptureLineageOps, CapturePage, CapturePublishOps, CaptureRemoval,
    CaptureStatus, CaptureSummary, Citations, Finding, IdentityOps, Limits, Lineage, LineageGraph,
    Me, NewFinding, NewProject, ParentsEdit, Project, ProjectCrudOps, ProjectPolicyOps,
    ProjectUpdate, PublishCaptureRequest, PublishMember, RecaptureSettings, RecaptureSettingsPatch,
    Role,
    record::{
        Appearance, CaptureRecord, Display, Environment as RecordEnvironment, Host, Member,
        MemberRole, Platform, PlatformFamily, RECORD_FORMAT, RecordKind, ScenarioRef, Software,
    },
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt::Write as _};
use uuid::Uuid;

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
