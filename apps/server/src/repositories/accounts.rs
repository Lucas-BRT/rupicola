//! The `accounts` table.

use sqlx::SqliteExecutor;

pub async fn password_hash(
    db: impl SqliteExecutor<'_>,
    handle: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT password_hash FROM accounts WHERE handle = ?1")
        .bind(handle)
        .fetch_optional(db)
        .await
}

pub async fn set_password(
    db: impl SqliteExecutor<'_>,
    handle: &str,
    password_hash: &str,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE accounts SET password_hash = ?1 WHERE handle = ?2")
        .bind(password_hash)
        .bind(handle)
        .execute(db)
        .await
        .map(drop)
}

pub async fn exists(db: impl SqliteExecutor<'_>, handle: &str) -> sqlx::Result<bool> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM accounts WHERE handle = ?1")
        .bind(handle)
        .fetch_optional(db)
        .await?;
    Ok(found.is_some())
}

/// Creates an ordinary account; `false` when the handle is taken.
pub async fn create(
    db: impl SqliteExecutor<'_>,
    handle: &str,
    password_hash: &str,
    now: i64,
) -> sqlx::Result<bool> {
    let created = sqlx::query(
        "INSERT INTO accounts (handle, password_hash, admin, created_at) VALUES (?1, ?2, 0, ?3)
         ON CONFLICT(handle) DO NOTHING",
    )
    .bind(handle)
    .bind(password_hash)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected();
    Ok(created > 0)
}

pub async fn admin_exists(db: impl SqliteExecutor<'_>) -> sqlx::Result<bool> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM accounts WHERE admin = 1 LIMIT 1")
        .fetch_optional(db)
        .await?;
    Ok(found.is_some())
}

/// Creates the first administrator; `false` when one already exists.
pub async fn create_first_admin(
    db: impl SqliteExecutor<'_>,
    handle: &str,
    password_hash: &str,
    now: i64,
) -> sqlx::Result<bool> {
    let created = sqlx::query(
        "INSERT OR IGNORE INTO accounts (handle, password_hash, admin, created_at)
         SELECT ?1, ?2, 1, ?3 WHERE NOT EXISTS (SELECT 1 FROM accounts WHERE admin = 1)",
    )
    .bind(handle)
    .bind(password_hash)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected();
    Ok(created > 0)
}
