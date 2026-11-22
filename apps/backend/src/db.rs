//! SQLite connection pool and schema migrations.
//!
//! Migrations are the numbered `.sql` files in `apps/backend/migrations/`, embedded
//! at build time by `sqlx::migrate!` and recorded (with checksums) in
//! `_sqlx_migrations`. Never edit a file that has shipped: add a new one.

use anyhow::Context;
use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{path::Path, time::Duration};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Opens (creating if needed) the database at `path` and applies pending migrations.
/// Every pooled connection enforces foreign keys and uses WAL, so readers never
/// block the writer.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or a migration fails.
pub async fn open(path: &Path) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .with_context(|| format!("cannot open {}", path.display()))?;
    MIGRATOR
        .run(&pool)
        .await
        .context("database migration failed")?;
    Ok(pool)
}

#[cfg(test)]
pub(crate) async fn open_test() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = open(&dir.path().join("panel.db")).await.unwrap();
    (dir, pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reopening_applies_nothing_twice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("panel.db");
        open(&path).await.unwrap().close().await;
        let pool = open(&path).await.unwrap();
        let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(applied, i64::try_from(MIGRATOR.iter().count()).unwrap());
    }

    #[tokio::test]
    async fn foreign_keys_and_checks_are_enforced() {
        let (_dir, db) = open_test().await;
        let orphan = sqlx::query(
            "INSERT INTO members (project_id, handle, role) VALUES ('nope', 'nobody', 'reader')",
        )
        .execute(&db)
        .await;
        assert!(orphan.is_err(), "orphan member was accepted");
        sqlx::raw_sql(
            "INSERT INTO accounts VALUES ('ana', 'hash', 0, 0);
             INSERT INTO projects VALUES ('demo', 'Demo', 0);
             INSERT INTO members VALUES ('demo', 'ana', 'owner');",
        )
        .execute(&db)
        .await
        .unwrap();
        let bad_role = sqlx::query("UPDATE members SET role = 'king' WHERE handle = 'ana'")
            .execute(&db)
            .await;
        assert!(bad_role.is_err(), "unknown role was accepted");
        sqlx::query("DELETE FROM projects WHERE id = 'demo'")
            .execute(&db)
            .await
            .unwrap();
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM members")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(left, 0, "deleting a project must remove its members");
    }

    #[tokio::test]
    async fn a_device_token_needs_a_project_and_no_person() {
        let (_dir, db) = open_test().await;
        sqlx::raw_sql(
            "INSERT INTO accounts VALUES ('ana', 'hash', 0, 0);
             INSERT INTO projects VALUES ('demo', 'Demo', 0);",
        )
        .execute(&db)
        .await
        .unwrap();
        let mixed = sqlx::query(
            "INSERT INTO tokens (digest, kind, handle, project_id, label, created_at)
             VALUES ('d', 'device', 'ana', 'demo', 'ci', 0)",
        )
        .execute(&db)
        .await;
        assert!(
            mixed.is_err(),
            "device token owned by a person was accepted"
        );
    }
}
