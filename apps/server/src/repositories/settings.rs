//! The `server_settings` and `project_settings` tables.

use sqlx::SqliteExecutor;

/// A stored setting, as text.
#[derive(sqlx::FromRow)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
}

pub async fn server(db: impl SqliteExecutor<'_>) -> sqlx::Result<Vec<SettingRow>> {
    sqlx::query_as("SELECT key, value FROM server_settings")
        .fetch_all(db)
        .await
}

pub async fn project(db: impl SqliteExecutor<'_>, project: &str) -> sqlx::Result<Vec<SettingRow>> {
    sqlx::query_as("SELECT key, value FROM project_settings WHERE project_id = ?1")
        .bind(project)
        .fetch_all(db)
        .await
}

pub async fn set_server(
    db: impl SqliteExecutor<'_>,
    key: &str,
    value: &str,
    actor: &str,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO server_settings (key, value, generation, updated_by, updated_at)
         VALUES (?1, ?2, 0, ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, generation = generation + 1,
           updated_by = excluded.updated_by, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(actor)
    .bind(now)
    .execute(db)
    .await
    .map(drop)
}

pub async fn reset_server(db: impl SqliteExecutor<'_>, key: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM server_settings WHERE key = ?1")
        .bind(key)
        .execute(db)
        .await
        .map(drop)
}

pub async fn set_project(
    db: impl SqliteExecutor<'_>,
    project: &str,
    key: &str,
    value: &str,
    actor: &str,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO project_settings (project_id, key, value, generation, updated_by, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(project_id, key) DO UPDATE SET value = excluded.value,
           generation = generation + 1, updated_by = excluded.updated_by,
           updated_at = excluded.updated_at",
    )
    .bind(project)
    .bind(key)
    .bind(value)
    .bind(actor)
    .bind(now)
    .execute(db)
    .await
    .map(drop)
}

pub async fn reset_project(
    db: impl SqliteExecutor<'_>,
    project: &str,
    key: &str,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM project_settings WHERE project_id = ?1 AND key = ?2")
        .bind(project)
        .bind(key)
        .execute(db)
        .await
        .map(drop)
}
