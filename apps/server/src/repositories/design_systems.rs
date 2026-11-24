//! SQLite queries for `design_versions` and `design_adoption`.

use crate::{
    domain::{ApiError, design_system::Stored, secret::now_ms},
    repositories::corrupt,
};
use rupicola_api::SystemItem;
use sqlx::{SqliteConnection, SqliteExecutor};

/// Versions the list shows.
const LIST_LIMIT: i64 = 100;

const SAVE: &str = "UPDATE design_versions
    SET generation = generation + 1, name = ?1, phase = ?2, document = ?3, notes = ?4,
        report = ?5
    WHERE id = ?6 AND project_id = ?7 AND generation = ?8";

#[derive(sqlx::FromRow)]
struct ListRow {
    id: String,
    name: String,
    phase: String,
}

#[derive(sqlx::FromRow)]
struct ReadRow {
    parent_id: Option<String>,
    author: String,
    phase: String,
    generation: i64,
    document: String,
    notes: String,
    report: Option<String>,
}

/// The version of `project` that is adopted.
pub async fn adopted(
    db: impl SqliteExecutor<'_>,
    project: &str,
) -> Result<Option<String>, ApiError> {
    let version =
        sqlx::query_scalar("SELECT version_id FROM design_adoption WHERE project_id = ?1")
            .bind(project)
            .fetch_optional(db)
            .await?;
    Ok(version)
}

/// Lists the newest versions of `project`.
pub async fn list(db: impl SqliteExecutor<'_>, project: &str) -> Result<Vec<SystemItem>, ApiError> {
    let rows: Vec<ListRow> = sqlx::query_as(
        "SELECT id, name, phase FROM design_versions WHERE project_id = ?1
         ORDER BY created_at DESC, rowid DESC LIMIT ?2",
    )
    .bind(project)
    .bind(LIST_LIMIT)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SystemItem {
                id: row.id,
                name: row.name,
                phase: row.phase.parse().map_err(corrupt)?,
            })
        })
        .collect()
}

/// Reads one design system version of `project`.
pub async fn read(
    db: impl SqliteExecutor<'_>,
    project: &str,
    id: &str,
) -> Result<Stored, ApiError> {
    let row: ReadRow = sqlx::query_as(
        "SELECT parent_id, author, phase, generation, document, notes, report
         FROM design_versions WHERE id = ?1 AND project_id = ?2",
    )
    .bind(id)
    .bind(project)
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Stored {
        id: id.to_owned(),
        generation: row.generation,
        parent: row.parent_id,
        author: row.author,
        phase: row.phase.parse().map_err(corrupt)?,
        document: serde_json::from_str(&row.document).map_err(corrupt)?,
        notes: serde_json::from_str(&row.notes).map_err(corrupt)?,
        report: row
            .report
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(corrupt)?,
    })
}

/// Inserts a new draft version.
pub async fn insert(
    db: impl SqliteExecutor<'_>,
    project: &str,
    version: &Stored,
) -> Result<(), ApiError> {
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
pub async fn save(
    db: impl SqliteExecutor<'_>,
    project: &str,
    version: &Stored,
) -> Result<(), ApiError> {
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

/// Saves `version` and records it as the project's adopted version inside `tx`.
pub async fn adopt(
    tx: &mut SqliteConnection,
    project: &str,
    version: &Stored,
) -> Result<(), ApiError> {
    save(&mut *tx, project, version).await?;
    sqlx::query(
        "INSERT INTO design_adoption (project_id, version_id) VALUES (?1, ?2)
         ON CONFLICT(project_id) DO UPDATE SET version_id = excluded.version_id",
    )
    .bind(project)
    .bind(&version.id)
    .execute(tx)
    .await?;
    Ok(())
}
