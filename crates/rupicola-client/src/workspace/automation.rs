use super::WorkspaceClient;
use crate::{Api, ClientError, Result};
use async_trait::async_trait;
use rupicola_api::{
    ArtifactBytes, BlobTransferOps, Capture, Job, JobFailure, JobProgress, JobQueueOps, JobResult,
    JobWorkerOps, Missing, MissingQuery, NewJob, OfflineCatalogOps, OfflineScanOps, Scenario,
    ScenarioOps, ScenarioSummary, ScenarioVersion, record::CaptureRecord,
};
use serde_json::Value;
use std::path::Path;

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
