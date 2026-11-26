use super::WorkspaceClient;
use crate::{Api, Result};
use async_trait::async_trait;
use base64::Engine as _;
use rupicola_api::{
    ArtifactChunk, ArtifactReadOps, Capture, CaptureCapabilities, CaptureCatalogOps, CaptureDiff,
    CaptureLifecycleOps, CaptureLineageOps, CapturePage, CapturePublishOps, CaptureRemoval,
    CaptureSummary, Citations, Finding, IdentityOps, Limits, Lineage, LineageGraph, Me, NewFinding,
    NewProject, ParentsEdit, Project, ProjectCrudOps, ProjectPolicyOps, ProjectUpdate,
    PublishCaptureRequest, RecaptureSettings, RecaptureSettingsPatch,
};
use serde_json::json;

fn parse_total_from_range(header: Option<&str>, fallback: usize) -> u64 {
    header
        .and_then(|v| v.rsplit_once('/'))
        .and_then(|(_, total)| total.parse::<u64>().ok())
        .unwrap_or_else(|| u64::try_from(fallback).unwrap_or(u64::MAX))
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
        self.call_mcp("capture_publish", json!(req)).await
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
