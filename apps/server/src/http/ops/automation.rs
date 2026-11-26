use super::ServerOps;
use crate::{
    domain::ApiError,
    services::{jobs, scenarios},
};
use async_trait::async_trait;
use rupicola_api::{
    Job, JobFailure, JobProgress, JobQueueOps, JobResult, JobWorkerOps, NewJob, OfflineCatalogOps,
    OfflineScanOps, Scenario, ScenarioOps, ScenarioSummary, ScenarioVersion,
};
use serde_json::Value;
use std::path::Path;

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
