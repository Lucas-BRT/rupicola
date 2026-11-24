//! The `tokens` table: every credential, stored as the SHA-256 of its secret.

use sqlx::SqliteExecutor;

/// The live token with a digest, and what its owner is.
#[derive(sqlx::FromRow)]
pub struct Bearer {
    pub kind: String,
    pub handle: Option<String>,
    pub project_id: Option<String>,
    pub admin: bool,
}

pub async fn bearer(
    db: impl SqliteExecutor<'_>,
    digest: &str,
    now: i64,
) -> sqlx::Result<Option<Bearer>> {
    sqlx::query_as(
        "SELECT t.kind, t.handle, t.project_id, COALESCE(a.admin, 0) AS admin FROM tokens t
         LEFT JOIN accounts a ON a.handle = t.handle
         WHERE t.digest = ?1 AND (t.expires_at IS NULL OR t.expires_at > ?2)",
    )
    .bind(digest)
    .bind(now)
    .fetch_optional(db)
    .await
}

pub async fn touch(db: impl SqliteExecutor<'_>, digest: &str, now: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE tokens SET last_used_at = ?1 WHERE digest = ?2")
        .bind(now)
        .bind(digest)
        .execute(db)
        .await
        .map(drop)
}

pub async fn purge_expired(db: impl SqliteExecutor<'_>, now: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM tokens WHERE expires_at IS NOT NULL AND expires_at <= ?1")
        .bind(now)
        .execute(db)
        .await
        .map(drop)
}

pub async fn insert_session(
    db: impl SqliteExecutor<'_>,
    digest: &str,
    handle: &str,
    created_at: i64,
    expires_at: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tokens (digest, kind, handle, label, created_at, expires_at)
         VALUES (?1, 'session', ?2, 'Session', ?3, ?4)",
    )
    .bind(digest)
    .bind(handle)
    .bind(created_at)
    .bind(expires_at)
    .execute(db)
    .await
    .map(drop)
}

pub async fn delete(db: impl SqliteExecutor<'_>, digest: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM tokens WHERE digest = ?1")
        .bind(digest)
        .execute(db)
        .await
        .map(drop)
}

/// Ends every session of `handle` except the one with digest `keep`.
pub async fn delete_other_sessions(
    db: impl SqliteExecutor<'_>,
    handle: &str,
    keep: &str,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM tokens WHERE kind = 'session' AND handle = ?1 AND digest <> ?2")
        .bind(handle)
        .bind(keep)
        .execute(db)
        .await
        .map(drop)
}
