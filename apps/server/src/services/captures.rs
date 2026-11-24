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
        access::{require_publisher, require_reader},
        limits,
    },
    state::{AppState, Shared},
};
use rupicola_api::{
    Capture, CaptureSummary, Finding, Missing, MissingQuery, NewCapture, NewFinding, Role,
};

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

/// Devices and reviewers publish a capture whose files are already stored.
pub async fn create(
    state: &Shared,
    principal: &Principal,
    project: &str,
    form: NewCapture,
) -> Result<Capture, ApiError> {
    require_publisher(state, principal, project, Role::Reviewer).await?;
    check_scenario(&form.scenario)?;
    let limits = limits::effective(state, project).await?;
    let max_artifacts = usize::try_from(limits.max_artifacts_per_capture).unwrap_or(usize::MAX);
    let planned = plan(&form, max_artifacts)?;
    let hashes: Vec<String> = planned
        .iter()
        .map(|item| item.file.sha256.clone())
        .collect();
    if !blobs::missing(&state.db, project, &hashes)
        .await?
        .is_empty()
    {
        return Err(ApiError::invalid("upload every file to this project first"));
    }
    let shared = state.clone();
    let sizes = tokio::task::spawn_blocking(move || {
        hashes
            .into_iter()
            .map(|sha256| {
                shared
                    .blobs
                    .size(&sha256)
                    .map_err(anyhow::Error::from)?
                    .ok_or_else(|| ApiError::invalid("upload every file to this project first"))
            })
            .collect::<Result<Vec<u64>, ApiError>>()
    })
    .await
    .map_err(anyhow::Error::from)??;
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
