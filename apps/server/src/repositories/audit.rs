//! The `audit_events` table.

use crate::domain::secret::now_ms;
use rupicola_api::AuditEvent;
use sqlx::SqliteExecutor;

/// An `audit_events` row.
#[derive(sqlx::FromRow)]
struct AuditRow {
    id: String,
    actor: String,
    event: String,
    occurred_at: i64,
}

impl From<AuditRow> for AuditEvent {
    fn from(row: AuditRow) -> Self {
        Self {
            id: row.id,
            actor: row.actor,
            event: row.event,
            occurred_at: row.occurred_at,
        }
    }
}

/// Appends an audit event, inside the caller's transaction when given one.
pub async fn record(
    db: impl SqliteExecutor<'_>,
    project: Option<&str>,
    actor: &str,
    event: &str,
) -> sqlx::Result<()> {
    record_with_detail(db, project, actor, event, None).await
}

/// Appends an audit event with an optional JSON detail payload.
pub async fn record_with_detail(
    db: impl SqliteExecutor<'_>,
    project: Option<&str>,
    actor: &str,
    event: &str,
    detail: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO audit_events (id, project_id, actor, event, occurred_at, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(project)
    .bind(actor)
    .bind(event)
    .bind(now_ms())
    .bind(detail)
    .execute(db)
    .await
    .map(drop)
}

/// The latest `limit` events of a project, newest first.
pub async fn latest(
    db: impl SqliteExecutor<'_>,
    project: &str,
    limit: i64,
) -> sqlx::Result<Vec<AuditEvent>> {
    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT id, actor, event, occurred_at FROM audit_events
         WHERE project_id = ?1 ORDER BY occurred_at DESC, id DESC LIMIT ?2",
    )
    .bind(project)
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(AuditEvent::from).collect())
}
