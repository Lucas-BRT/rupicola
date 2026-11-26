//! `/projects/{id}/jobs` and `/jobs/{id}` handlers.

use crate::{
    domain::{ApiError, Principal},
    services::jobs,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rupicola_api::{Job, JobFailure, JobProgress, JobResult, NewJob};
use serde::Deserialize;

/// `GET /v1/projects/{id}/jobs?open=true`
#[derive(Deserialize)]
pub struct ListQuery {
    /// Only the jobs still waiting or running.
    #[serde(default)]
    open: bool,
}

/// `POST /v1/projects/{id}/jobs`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewJob>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    jobs::create(&state, &principal, &project, &form)
        .await
        .map(|job| (StatusCode::CREATED, Json(job)))
}

/// `GET /v1/projects/{id}/jobs`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Job>>, ApiError> {
    jobs::list(&state, &principal, &project, query.open)
        .await
        .map(Json)
}

/// `POST /v1/projects/{id}/jobs/claim`: 200 with the job the caller now holds, or 204 when
/// there is nothing to do.
pub async fn claim(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Response, ApiError> {
    Ok(match jobs::claim(&state, &principal, &project).await? {
        Some(job) => Json(job).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}

/// `GET /v1/jobs/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Job>, ApiError> {
    jobs::get(&state, &principal, &id).await.map(Json)
}

/// `POST /v1/jobs/{id}/progress`
pub async fn progress(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(progress): Json<JobProgress>,
) -> Result<Json<Job>, ApiError> {
    jobs::progress(&state, &principal, &id, progress)
        .await
        .map(Json)
}

/// `POST /v1/jobs/{id}/complete`
pub async fn complete(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(result): Json<JobResult>,
) -> Result<Json<Job>, ApiError> {
    jobs::complete(&state, &principal, &id, &result)
        .await
        .map(Json)
}

/// `POST /v1/jobs/{id}/fail`
pub async fn fail(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(failure): Json<JobFailure>,
) -> Result<Json<Job>, ApiError> {
    jobs::fail(&state, &principal, &id, &failure)
        .await
        .map(Json)
}

/// `POST /v1/jobs/{id}/cancel`
pub async fn cancel(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Job>, ApiError> {
    jobs::cancel(&state, &principal, &id).await.map(Json)
}

#[cfg(test)]
mod tests;
