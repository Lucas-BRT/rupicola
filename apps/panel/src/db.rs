//! Versioned schema migrations for `panel.db`.
//!
//! Each entry runs once, inside one transaction, and is recorded in
//! `schema_migrations`. Never edit an entry that has shipped: append a new version.

use crate::BoxError;
use turso::{Connection, Value, transaction::TransactionBehavior};

const MIGRATIONS: &[(i64, &[&str])] = &[(
    1,
    &[
        "CREATE TABLE captures (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            project_label TEXT NOT NULL,
            scenario TEXT NOT NULL,
            revision TEXT,
            status TEXT NOT NULL,
            blocker TEXT,
            inspection TEXT NOT NULL,
            environment_json TEXT NOT NULL,
            findings_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            legacy_root TEXT,
            legacy_name TEXT
        )",
        "CREATE INDEX idx_captures_project ON captures(project_id, created_at_ms)",
        "CREATE UNIQUE INDEX idx_captures_legacy ON captures(legacy_root, legacy_name)",
        "CREATE TABLE artifacts (
            id TEXT PRIMARY KEY,
            capture_id TEXT NOT NULL,
            project_id TEXT NOT NULL,
            role TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            position INTEGER NOT NULL,
            path TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            bytes INTEGER NOT NULL,
            mime TEXT NOT NULL,
            target_ms REAL,
            video_ms REAL
        )",
        "CREATE UNIQUE INDEX idx_artifacts_path ON artifacts(capture_id, path)",
        "CREATE INDEX idx_artifacts_sha ON artifacts(sha256)",
    ],
)];

/// Applies every migration newer than the recorded schema version.
///
/// # Errors
///
/// Returns an error if reading the version or running a migration fails; a failed
/// migration rolls back and leaves the previous version in place.
pub async fn migrate(db: &Connection) -> Result<(), BoxError> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        (),
    )
    .await?;
    let current = version(db).await?;
    for (target, statements) in MIGRATIONS.iter().filter(|(target, _)| *target > current) {
        let mut batch: Vec<(&str, Vec<Value>)> = statements
            .iter()
            .map(|statement| (*statement, Vec::new()))
            .collect();
        batch.push((
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            vec![Value::Integer(*target), Value::Integer(crate::auth::now())],
        ));
        db.transactional_batch(batch, TransactionBehavior::Immediate)
            .await?;
    }
    Ok(())
}

async fn version(db: &Connection) -> Result<i64, BoxError> {
    let mut rows = db
        .query(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            (),
        )
        .await?;
    Ok(match rows.next().await? {
        Some(row) => row.get(0)?,
        None => 0,
    })
}

#[cfg(test)]
pub(crate) async fn test_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("panel.db");
    let db = turso::Builder::new_local(path.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();
    (dir, conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrate_is_idempotent_and_records_the_latest_version() {
        let (_dir, db) = test_db().await;
        migrate(&db).await.unwrap();
        migrate(&db).await.unwrap();
        let latest = MIGRATIONS.last().map(|(v, _)| *v).unwrap();
        assert_eq!(version(&db).await.unwrap(), latest);
        let mut rows = db
            .query("SELECT COUNT(*) FROM schema_migrations", ())
            .await
            .unwrap();
        let count: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(count, i64::try_from(MIGRATIONS.len()).unwrap());
    }
}
