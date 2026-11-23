//! SQLite queries for `design_versions` and `design_adoption`.

use crate::domain::{ApiError, design_system::Stored, secret::now_ms};
use rupicola_api::SystemItem;
use sqlx::{Row, SqlitePool};

/// Versions the list shows.
const LIST_LIMIT: i64 = 100;

const SAVE: &str = "UPDATE design_versions
    SET generation = generation + 1, name = ?1, phase = ?2, document = ?3, notes = ?4,
        report = ?5
    WHERE id = ?6 AND project_id = ?7 AND generation = ?8";

fn corrupt(error: impl Into<anyhow::Error>) -> ApiError {
    ApiError::Internal(error.into())
}

/// The version of `project` that is adopted.
pub async fn adopted(db: &SqlitePool, project: &str) -> Result<Option<String>, ApiError> {
    let version =
        sqlx::query_scalar("SELECT version_id FROM design_adoption WHERE project_id = ?1")
            .bind(project)
            .fetch_optional(db)
            .await?;
    Ok(version)
}

/// Lists the newest versions of `project`.
pub async fn list(db: &SqlitePool, project: &str) -> Result<Vec<SystemItem>, ApiError> {
    let rows = sqlx::query(
        "SELECT id, name, phase FROM design_versions WHERE project_id = ?1
         ORDER BY created_at DESC, rowid DESC LIMIT ?2",
    )
    .bind(project)
    .bind(LIST_LIMIT)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            let phase: String = row.get("phase");
            Ok(SystemItem {
                id: row.get("id"),
                name: row.get("name"),
                phase: phase.parse().map_err(corrupt)?,
            })
        })
        .collect()
}

/// Reads one design system version of `project`.
pub async fn read(db: &SqlitePool, project: &str, id: &str) -> Result<Stored, ApiError> {
    let row = sqlx::query(
        "SELECT parent_id, author, phase, generation, document, notes, report
         FROM design_versions WHERE id = ?1 AND project_id = ?2",
    )
    .bind(id)
    .bind(project)
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let phase: String = row.get("phase");
    let document: String = row.get("document");
    let notes: String = row.get("notes");
    let report: Option<String> = row.get("report");
    Ok(Stored {
        id: id.to_owned(),
        generation: row.get("generation"),
        parent: row.get("parent_id"),
        author: row.get("author"),
        phase: phase.parse().map_err(corrupt)?,
        document: serde_json::from_str(&document).map_err(corrupt)?,
        notes: serde_json::from_str(&notes).map_err(corrupt)?,
        report: report
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(corrupt)?,
    })
}

/// Inserts a new draft version.
pub async fn insert(db: &SqlitePool, project: &str, version: &Stored) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO design_versions
           (id, project_id, parent_id, author, phase, generation, name, document, notes, report,
            frozen_screen, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, NULL, NULL, ?9)",
    )
    .bind(&version.id)
    .bind(project)
    .bind(&version.parent)
    .bind(&version.author)
    .bind(version.phase.as_str())
    .bind(&version.document.name)
    .bind(serde_json::to_string(&version.document).map_err(corrupt)?)
    .bind(serde_json::to_string(&version.notes).map_err(corrupt)?)
    .bind(now_ms())
    .execute(db)
    .await?;
    Ok(())
}

/// Writes `version` if nobody changed it since it was read.
pub async fn save(db: &SqlitePool, project: &str, version: &Stored) -> Result<(), ApiError> {
    let written = sqlx::query(SAVE)
        .bind(&version.document.name)
        .bind(version.phase.as_str())
        .bind(serde_json::to_string(&version.document).map_err(corrupt)?)
        .bind(serde_json::to_string(&version.notes).map_err(corrupt)?)
        .bind(
            version
                .report
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(corrupt)?,
        )
        .bind(&version.id)
        .bind(project)
        .bind(version.generation)
        .execute(db)
        .await?
        .rows_affected();
    if written == 1 {
        Ok(())
    } else {
        Err(ApiError::Conflict("the version changed; reload it"))
    }
}

/// Saves `version` and records it as the project's adopted version in one transaction.
pub async fn adopt(db: &SqlitePool, project: &str, version: &Stored) -> Result<(), ApiError> {
    let mut tx = db.begin().await?;
    let written = sqlx::query(SAVE)
        .bind(&version.document.name)
        .bind(version.phase.as_str())
        .bind(serde_json::to_string(&version.document).map_err(corrupt)?)
        .bind(serde_json::to_string(&version.notes).map_err(corrupt)?)
        .bind(
            version
                .report
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(corrupt)?,
        )
        .bind(&version.id)
        .bind(project)
        .bind(version.generation)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if written != 1 {
        return Err(ApiError::Conflict("the version changed; reload it"));
    }
    sqlx::query(
        "INSERT INTO design_adoption (project_id, version_id) VALUES (?1, ?2)
         ON CONFLICT(project_id) DO UPDATE SET version_id = excluded.version_id",
    )
    .bind(project)
    .bind(&version.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
