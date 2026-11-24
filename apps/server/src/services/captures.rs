//! Publishing captures the old way, browsing and reviewing them, and the files they hold.

use crate::{
    domain::{
        ApiError, Principal,
        capture::{check_scenario, clean_note, plan},
        secret::now_ms,
    },
    repositories::{
        audit, blobs,
        captures::{self, ArtifactBlob},
    },
    services::{
        access::{access, require, require_publisher},
        limits,
    },
    state::{AppState, Shared},
};
use axum::body::Bytes;
use rupicola_api::{
    Capture, CaptureSummary, Finding, Missing, MissingQuery, NewCapture, NewFinding, Role,
};

/// Stores a file under its digest, for any signed-in caller; blobs nobody references are
/// pruned later. Returns whether it was new.
pub async fn put_blob(state: Shared, sha256: String, data: Bytes) -> Result<bool, ApiError> {
    tokio::task::spawn_blocking(move || state.blobs.put(&sha256, &data))
        .await
        .map_err(anyhow::Error::from)?
}

/// Whether a file is stored under `sha256`.
pub fn has_blob(state: &AppState, sha256: &str) -> Result<bool, ApiError> {
    Ok(state
        .blobs
        .size(sha256)
        .map_err(anyhow::Error::from)?
        .is_some())
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

/// Devices and reviewers publish a capture whose files are already stored.
pub async fn create(
    state: &AppState,
    principal: &Principal,
    project: &str,
    form: NewCapture,
) -> Result<Capture, ApiError> {
    require_publisher(state, principal, project, Role::Reviewer).await?;
    check_scenario(&form.scenario)?;
    let limits = limits::effective(state, project).await?;
    let max_artifacts = usize::try_from(limits.max_artifacts_per_capture).unwrap_or(usize::MAX);
    let planned = plan(&form, max_artifacts)?;
    let mut sizes = Vec::with_capacity(planned.len());
    for item in &planned {
        let size = state
            .blobs
            .size(&item.file.sha256)
            .map_err(anyhow::Error::from)?
            .ok_or(ApiError::invalid(
                "upload every file with PUT /v1/blobs/{sha256} first",
            ))?;
        sizes.push(size);
    }
    let id = uuid::Uuid::now_v7().to_string();
    let mut tx = state.db.begin().await?;
    captures::insert(&mut *tx, &id, project, &form, now_ms()).await?;
    for (position, (item, size)) in planned.iter().zip(&sizes).enumerate() {
        captures::insert_artifact(&mut *tx, &id, position, item, *size).await?;
        // The project now cites the file, so it is no longer missing for it.
        blobs::record(&mut tx, project, &item.file.sha256, *size).await?;
    }
    audit::record(
        &mut *tx,
        Some(project),
        principal.actor(),
        &format!("Published capture {}", form.scenario),
    )
    .await?;
    tx.commit().await?;
    captures::load(&state.db, &id).await
}

/// A project's live captures, newest first.
pub async fn list(
    state: &AppState,
    principal: &Principal,
    project: &str,
) -> Result<Vec<CaptureSummary>, ApiError> {
    require(state, principal, project, Role::Reader).await?;
    Ok(captures::of_project(&state.db, project).await?)
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
    require(state, principal, &project, role).await
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
    access(state, principal, &blob.project_id)
        .await?
        .require(Role::Reader)?;
    Ok(blob)
}
