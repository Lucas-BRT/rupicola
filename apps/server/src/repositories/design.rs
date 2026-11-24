//! The `design_entries` table.

use crate::domain::ApiError;
use rupicola_api::{DesignEntry, NewDesignEntry};
use sqlx::SqliteExecutor;

/// Columns of a [`Row`], in order.
const COLUMNS: &str = "id, kind, status, title, note, surface, supersedes, reference, evidence,
                       author, created_at";

/// A `design_entries` row.
#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    kind: String,
    status: String,
    title: String,
    note: String,
    surface: Option<String>,
    supersedes: Option<String>,
    reference: Option<String>,
    evidence: Option<String>,
    author: Option<String>,
    created_at: i64,
}

impl TryFrom<Row> for DesignEntry {
    type Error = ApiError;

    fn try_from(row: Row) -> Result<Self, ApiError> {
        Ok(Self {
            id: row.id,
            entry: NewDesignEntry {
                kind: row.kind.parse().map_err(anyhow::Error::from)?,
                status: row.status.parse().map_err(anyhow::Error::from)?,
                title: row.title,
                note: row.note,
                surface: row.surface,
                supersedes: row.supersedes,
                reference: row.reference,
                evidence: row.evidence,
            },
            author: row.author,
            created_at: row.created_at,
        })
    }
}

/// A project's records, newest first.
pub async fn list(
    db: impl SqliteExecutor<'_>,
    project: &str,
) -> Result<Vec<DesignEntry>, ApiError> {
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM design_entries WHERE project_id = ?1
         ORDER BY created_at DESC, id DESC"
    )))
    .bind(project)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(DesignEntry::try_from).collect()
}

pub async fn get(db: impl SqliteExecutor<'_>, id: &str) -> Result<DesignEntry, ApiError> {
    let row: Row = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM design_entries WHERE id = ?1"
    )))
    .bind(id)
    .fetch_one(db)
    .await?;
    row.try_into()
}

/// Whether `id` is a record of `project`.
pub async fn belongs(db: impl SqliteExecutor<'_>, id: &str, project: &str) -> sqlx::Result<bool> {
    let known: Option<String> =
        sqlx::query_scalar("SELECT id FROM design_entries WHERE id = ?1 AND project_id = ?2")
            .bind(id)
            .bind(project)
            .fetch_optional(db)
            .await?;
    Ok(known.is_some())
}

pub async fn insert(
    db: impl SqliteExecutor<'_>,
    id: &str,
    project: &str,
    form: &NewDesignEntry,
    author: Option<&str>,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO design_entries (id, project_id, kind, status, title, note, surface,
           supersedes, reference, evidence, author, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )
    .bind(id)
    .bind(project)
    .bind(form.kind.as_str())
    .bind(form.status.as_str())
    .bind(form.title.trim())
    .bind(&form.note)
    .bind(&form.surface)
    .bind(&form.supersedes)
    .bind(&form.reference)
    .bind(&form.evidence)
    .bind(author)
    .bind(now)
    .execute(db)
    .await
    .map(drop)
}
