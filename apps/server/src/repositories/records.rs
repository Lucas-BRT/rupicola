//! Writing a capture record: the capture, its files, its parents, its screens and its facets.

use crate::domain::ApiError;
use rupicola_api::record::{CaptureRecord, Member};
use sqlx::{SqliteConnection, SqliteExecutor};
use std::collections::HashMap;

/// The project and digest of the capture kept under `id`, if any.
#[derive(sqlx::FromRow)]
pub struct Kept {
    pub project_id: String,
    pub digest: Option<String>,
}

pub async fn kept(db: impl SqliteExecutor<'_>, id: &str) -> sqlx::Result<Option<Kept>> {
    sqlx::query_as("SELECT project_id, digest FROM captures WHERE id = ?1")
        .bind(id)
        .fetch_optional(db)
        .await
}

/// Of `hashes`, the files `project` holds, with their sizes.
pub async fn held(
    db: impl SqliteExecutor<'_>,
    project: &str,
    hashes: &[&str],
) -> Result<HashMap<String, i64>, ApiError> {
    Ok(sqlx::query_as(
        "SELECT b.sha256, b.bytes FROM project_blobs p JOIN blobs b ON b.sha256 = p.sha256
         WHERE p.project_id = ?1 AND p.sha256 IN (SELECT value FROM json_each(?2))",
    )
    .bind(project)
    .bind(serde_json::to_string(hashes).map_err(anyhow::Error::from)?)
    .fetch_all(db)
    .await?
    .into_iter()
    .collect())
}

/// Of `ids`, the live captures of `project`.
pub async fn live(
    db: impl SqliteExecutor<'_>,
    project: &str,
    ids: &[&str],
) -> Result<Vec<String>, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM captures WHERE project_id = ?1 AND deleted_at IS NULL
           AND id IN (SELECT value FROM json_each(?2))",
    )
    .bind(project)
    .bind(serde_json::to_string(ids).map_err(anyhow::Error::from)?)
    .fetch_all(db)
    .await?)
}

pub async fn insert(
    db: impl SqliteExecutor<'_>,
    project: &str,
    record: &CaptureRecord,
    digest: &str,
    now: i64,
) -> Result<(), ApiError> {
    let (plan_json, plan_digest) = match &record.plan_spec {
        Some(spec) => {
            let norm = spec.normalized();
            (
                Some(serde_json::to_string(&norm).map_err(anyhow::Error::from)?),
                Some(norm.digest()),
            )
        }
        None => (None, None),
    };
    sqlx::query(
        "INSERT INTO captures (id, project_id, scenario, revision, status, blocker, inspection,
           environment, created_at, kind, digest, series, captured_at, plan_json, plan_digest)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )
    .bind(&record.id)
    .bind(project)
    .bind(&record.scenario.name)
    .bind(&record.revision)
    .bind(record.status.as_str())
    .bind(&record.blocker)
    .bind(serde_json::to_string(&record.environment).map_err(anyhow::Error::from)?)
    .bind(now)
    .bind(record.kind.as_str())
    .bind(digest)
    .bind(&record.series)
    .bind(record.captured_at)
    .bind(&plan_json)
    .bind(&plan_digest)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn insert_member(
    db: impl SqliteExecutor<'_>,
    capture: &str,
    position: usize,
    member: &Member,
    surface: Option<&str>,
) -> Result<(), ApiError> {
    let locator = if member.locator.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&member.locator).map_err(anyhow::Error::from)?)
    };
    sqlx::query(
        "INSERT INTO artifacts (id, capture_id, role, owner_id, position, path, sha256, bytes,
           mime, target_ms, video_ms, surface_id, locator)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(capture)
    .bind(member.role.as_str())
    .bind(&member.owner)
    .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
    .bind(&member.path)
    .bind(&member.sha256)
    .bind(i64::try_from(member.bytes).map_err(anyhow::Error::from)?)
    .bind(crate::domain::blob::mime(&member.path))
    .bind(member.target_ms.and_then(|ms| i64::try_from(ms).ok()))
    .bind(member.video_ms.and_then(|ms| i64::try_from(ms).ok()))
    .bind(surface)
    .bind(locator)
    .execute(db)
    .await?;
    Ok(())
}

/// Records a parent the record itself names.
pub async fn insert_parent(
    db: impl SqliteExecutor<'_>,
    project: &str,
    capture: &str,
    parent: &str,
    kind: &str,
    position: usize,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO capture_parents (project_id, capture_id, parent_id, kind, position, origin)
         VALUES (?1, ?2, ?3, ?4, ?5, 'record')",
    )
    .bind(project)
    .bind(capture)
    .bind(parent)
    .bind(kind)
    .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn insert_facet(
    db: impl SqliteExecutor<'_>,
    project: &str,
    capture: &str,
    key: &str,
    value: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO capture_facets (project_id, key, value, capture_id) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(project)
    .bind(key)
    .bind(value)
    .bind(capture)
    .execute(db)
    .await
    .map(drop)
}

/// The id of the project's screen called `key`, made the first time it is named.
pub async fn surface_id(
    db: &mut SqliteConnection,
    project: &str,
    key: &str,
) -> sqlx::Result<String> {
    sqlx::query("INSERT OR IGNORE INTO surfaces (id, project_id, key) VALUES (?1, ?2, ?3)")
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(project)
        .bind(key)
        .execute(&mut *db)
        .await?;
    sqlx::query_scalar("SELECT id FROM surfaces WHERE project_id = ?1 AND key = ?2")
        .bind(project)
        .bind(key)
        .fetch_one(&mut *db)
        .await
}
