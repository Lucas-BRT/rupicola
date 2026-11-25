//! Browsing and reviewing captures, blob uploads, and OS capture capability probing.

use crate::{
    domain::{ApiError, Principal, capture::clean_note, secret::now_ms},
    repositories::{
        blobs,
        captures::{self, ArtifactBlob},
    },
    services::{
        access::{require_publisher, require_reader},
        limits,
    },
    state::{AppState, Shared},
};
use rupicola_api::{
    Capture, CaptureBackendStatus, CaptureCapabilities, CaptureSummary, Finding, Missing,
    MissingQuery, NewFinding, Role,
};
use rupicola_capture::asynchronous::probe_environment;

/// Probes host OS capture backend availability without installing any dependencies.
pub async fn capabilities() -> CaptureCapabilities {
    let reports = probe_environment().await;
    CaptureCapabilities {
        backends: reports
            .into_iter()
            .map(|report| CaptureBackendStatus {
                id: report.id,
                label: report.label,
                available: report.available,
                detected: (!report.detected_binaries.is_empty()).then(|| {
                    report
                        .detected_binaries
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
                install_hint: (!report.available).then_some(report.install_guidance),
            })
            .collect(),
    }
}

async fn put_blob(state: Shared, sha256: String, data: Vec<u8>) -> Result<bool, ApiError> {
    tokio::task::spawn_blocking(move || state.blobs.put(&sha256, &data))
        .await
        .map_err(anyhow::Error::from)?
}

/// Stores a file under its digest and records it as belonging to `project`.
pub async fn put_project_blob(
    state: &Shared,
    project: &str,
    sha256: String,
    data: Vec<u8>,
) -> Result<bool, ApiError> {
    let bytes = u64::try_from(data.len()).map_err(anyhow::Error::from)?;
    let created = put_blob(state.clone(), sha256.clone(), data).await?;
    let mut connection = state.db.acquire().await?;
    blobs::record(&mut connection, project, &sha256, bytes).await?;
    Ok(created)
}

/// Which of these files the project does not have yet, so a client uploads only those,
/// with the limits to upload them under.
pub async fn missing(
    state: &AppState,
    principal: &Principal,
    project: &str,
    query: &MissingQuery,
) -> Result<Missing, ApiError> {
    require_publisher(state, principal, project, Role::Reviewer).await?;
    query.validate().map_err(ApiError::invalid)?;
    Ok(Missing {
        missing: blobs::missing(&state.db, project, &query.sha256).await?,
        limits: limits::effective(state, project).await?,
    })
}

/// A project's live captures, newest first.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<CaptureSummary>, ApiError> {
    require_reader(state, principal, project).await?;
    captures::of_project(&state.db, project).await
}

/// Checks the caller's access to the project of `capture`.
pub async fn require_capture(
    state: &AppState,
    principal: &Principal,
    capture: &str,
    role: Role,
) -> Result<(), ApiError> {
    let project = captures::project_of(&state.db, capture)
        .await?
        .ok_or(ApiError::NotFound)?;
    require_publisher(state, principal, &project, role).await
}

/// Fails unless `artifact` is a file of one of `project`'s captures.
pub async fn require_project_artifact(
    state: &AppState,
    project: &str,
    artifact: &str,
) -> Result<(), ApiError> {
    if captures::artifact_in_project(&state.db, artifact, project).await? {
        Ok(())
    } else {
        Err(ApiError::invalid(
            "the evidence must be files of this project's captures",
        ))
    }
}

pub async fn get(state: &AppState, principal: &Principal, id: &str) -> Result<Capture, ApiError> {
    require_capture(state, principal, id, Role::Reader).await?;
    captures::load(&state.db, id).await
}

/// Reviewers note what they saw; the capture becomes reviewed.
pub async fn add_finding(
    state: &AppState,
    principal: &Principal,
    id: &str,
    form: NewFinding,
) -> Result<Finding, ApiError> {
    require_capture(state, principal, id, Role::Reviewer).await?;
    let note = clean_note(&form.note)?;
    if let Some(evidence) = &form.evidence_artifact_id
        && !captures::artifact_in_capture(&state.db, evidence, id).await?
    {
        return Err(ApiError::invalid(
            "the evidence must be a file of this capture",
        ));
    }
    let finding = Finding {
        id: uuid::Uuid::now_v7().to_string(),
        note: note.to_owned(),
        evidence_artifact_id: form.evidence_artifact_id,
        author: principal.handle.clone(),
        created_at: now_ms(),
    };
    let mut tx = state.db.begin().await?;
    captures::insert_finding(&mut *tx, id, &finding).await?;
    captures::mark_reviewed(&mut *tx, id).await?;
    tx.commit().await?;
    Ok(finding)
}

/// Digest, media type and size of an artifact, for readers of its project.
pub async fn artifact(
    state: &AppState,
    principal: &Principal,
    id: &str,
) -> Result<ArtifactBlob, ApiError> {
    let blob = captures::artifact_blob(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    require_reader(state, principal, &blob.project_id).await?;
    Ok(blob)
}

/// Reads a byte range of an artifact on a blocking thread, for readers of its project.
pub async fn read_artifact_range(
    state: &Shared,
    principal: &Principal,
    id: &str,
    offset: u64,
    length: usize,
) -> Result<(ArtifactBlob, Vec<u8>), ApiError> {
    let blob = artifact(state, principal, id).await?;
    let sha256 = blob.sha256.clone();
    let shared = state.clone();
    let data =
        tokio::task::spawn_blocking(move || shared.blobs.read_range(&sha256, offset, length))
            .await
            .map_err(anyhow::Error::from)??;
    Ok((blob, data))
}
