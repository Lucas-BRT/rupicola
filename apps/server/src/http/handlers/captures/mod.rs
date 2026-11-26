//! `/projects/{id}/captures`, `/captures/{id}`, `/artifacts/{id}` and project blob uploads.

use crate::{
    domain::{ApiError, Principal},
    services::{captures, uploads::Receiving},
    state::Shared,
};
use axum::{
    Json,
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    response::Response,
};
use rupicola_api::{
    Capture, CaptureCapabilities, CaptureSummary, Finding, Missing, MissingQuery, NewFinding,
};

/// 201 for a file stored now, 200 for one already kept.
const fn stored(created: bool) -> StatusCode {
    if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    }
}

/// `GET /v1/capture-capabilities`
pub async fn capabilities(_principal: Principal) -> Json<CaptureCapabilities> {
    Json(captures::capabilities().await)
}

/// `PUT /v1/projects/{id}/blobs/{sha256}`: the file arrives as a stream, cut when it takes
/// longer than the project allows in all or between two bytes.
pub async fn upload(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, sha256)): Path<(String, String)>,
    mut body: Body,
) -> Result<StatusCode, ApiError> {
    use http_body_util::BodyExt as _;
    let mut receiving = Receiving::begin(&state, &principal, &project, &sha256).await?;
    let deadline = tokio::time::Instant::now() + receiving.timeout();
    let idle = receiving.idle();
    loop {
        let wait = idle.min(deadline.saturating_duration_since(tokio::time::Instant::now()));
        let frame = match tokio::time::timeout(wait, body.frame()).await {
            Err(_) => return Err(ApiError::Timeout),
            Ok(None) => break,
            Ok(Some(frame)) => {
                frame.map_err(|e| ApiError::invalid(format!("the upload broke off: {e}")))?
            }
        };
        if let Ok(chunk) = frame.into_data() {
            receiving.take(&chunk).await?;
        }
    }
    receiving.finish(&state).await.map(stored)
}

/// `POST /v1/projects/{id}/blobs/missing`
pub async fn missing(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(query): Json<MissingQuery>,
) -> Result<Json<Missing>, ApiError> {
    captures::missing(&state, &principal, &project, &query)
        .await
        .map(Json)
}

/// `GET /v1/projects/{id}/captures`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<CaptureSummary>>, ApiError> {
    captures::list(&state, &principal, &project).await.map(Json)
}

/// `GET /v1/captures/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Capture>, ApiError> {
    captures::get(&state, &principal, &id).await.map(Json)
}

/// `POST /v1/captures/{id}/findings`
pub async fn add_finding(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<NewFinding>,
) -> Result<(StatusCode, Json<Finding>), ApiError> {
    let finding = captures::add_finding(&state, &principal, &id, form).await?;
    Ok((StatusCode::CREATED, Json(finding)))
}

/// `GET /v1/artifacts/{id}`: the file's bytes, for readers of its project.
pub async fn artifact(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    let blob = captures::artifact(&state, &principal, &id).await?;
    crate::http::blob::respond(&state, blob.sha256, &blob.mime, request).await
}

#[cfg(test)]
mod tests;
