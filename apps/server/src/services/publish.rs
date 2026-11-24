//! Publishing a capture record: the client names the capture, the server checks the record and
//! keeps it once. Sending the same record again is harmless; sending other content under the
//! same id is refused.

use crate::{
    domain::{
        ApiError, Principal,
        record::{check_address, check_files, check_members, check_parents},
        secret::now_ms,
    },
    repositories::{audit, captures, records},
    services::{access::require_publisher, limits},
    state::AppState,
};
use rupicola_api::{Capture, Limits, Role, record::CaptureRecord};
use sqlx::SqliteConnection;

/// What publishing did.
pub enum Published {
    /// The record is kept now.
    New(Capture),
    /// The same record was kept before.
    Same(Capture),
}

/// Keeps the capture record, whose files the project must already hold.
pub async fn publish(
    state: &AppState,
    principal: &Principal,
    project: &str,
    id: &str,
    record: &CaptureRecord,
) -> Result<Published, ApiError> {
    require_publisher(state, principal, project, Role::Reviewer).await?;
    check_address(record, project, id)?;
    let digest = record
        .digest()
        .map_err(|error| ApiError::invalid(error.to_string()))?;
    let limits = limits::effective(state, project).await?;
    let mut tx = state.db.begin().await?;
    if let Some(kept) = records::kept(&mut *tx, id).await? {
        tx.commit().await?;
        return if kept.project_id == project && kept.digest.as_deref() == Some(digest.as_str()) {
            Ok(Published::Same(captures::load(&state.db, id).await?))
        } else {
            Err(ApiError::Conflict(
                "a capture with this id already holds other content",
            ))
        };
    }
    check(&mut tx, &limits, project, record).await?;
    keep(&mut tx, principal, project, record, &digest).await?;
    tx.commit().await?;
    Ok(Published::New(captures::load(&state.db, id).await?))
}

/// Checks what only the server knows: the project's limits, the files it holds, and that the
/// parents are its captures.
async fn check(
    tx: &mut SqliteConnection,
    limits: &Limits,
    project: &str,
    record: &CaptureRecord,
) -> Result<(), ApiError> {
    check_members(
        record,
        usize::try_from(limits.max_artifacts_per_capture).unwrap_or(usize::MAX),
    )?;
    let hashes: Vec<&str> = record.members.iter().map(|m| m.sha256.as_str()).collect();
    check_files(record, &records::held(&mut *tx, project, &hashes).await?)?;
    let parents: Vec<&str> = record.parents.iter().map(|p| p.id.as_str()).collect();
    check_parents(&parents, &records::live(&mut *tx, project, &parents).await?)
}

/// Writes the capture, its files, its parents, its screens and its facets in one transaction.
async fn keep(
    tx: &mut SqliteConnection,
    principal: &Principal,
    project: &str,
    record: &CaptureRecord,
    digest: &str,
) -> Result<(), ApiError> {
    records::insert(&mut *tx, project, record, digest, now_ms()).await?;
    for (position, member) in record.members.iter().enumerate() {
        let surface = match &member.surface {
            Some(key) => Some(records::surface_id(tx, project, key).await?),
            None => None,
        };
        records::insert_member(&mut *tx, &record.id, position, member, surface.as_deref()).await?;
    }
    for (position, parent) in record.parents.iter().enumerate() {
        records::insert_parent(
            &mut *tx,
            project,
            &record.id,
            &parent.id,
            parent.kind.as_str(),
            position,
        )
        .await?;
    }
    for (key, value) in record.environment.facets() {
        records::insert_facet(&mut *tx, project, &record.id, &key, &value).await?;
    }
    audit::record(
        &mut *tx,
        Some(project),
        principal.actor(),
        &format!("Published capture {}", record.scenario.name),
    )
    .await?;
    Ok(())
}
