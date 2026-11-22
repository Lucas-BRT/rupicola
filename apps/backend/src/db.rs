//! Versioned schema migrations for `panel.db`, read from the numbered `.sql` files
//! in `apps/backend/migrations/`.
//!
//! Each file runs once, inside one transaction, and is recorded in
//! `schema_migrations`. Never edit a file that has shipped: add a new one.

use anyhow::Context;
use turso::{Connection, Value, transaction::TransactionBehavior};

/// Migration files in `apps/backend/migrations/`, embedded at build time.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/0001_init.sql"))];

/// Splits a migration file into statements: `--` comment lines are dropped and
/// statements end with `;`. Migration files must not put `;` inside literals.
fn statements(sql: &str) -> Vec<String> {
    sql.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Applies every migration newer than the recorded schema version.
///
/// # Errors
///
/// Returns an error if reading the version or running a migration fails; a failed
/// migration rolls back and leaves the previous version in place.
pub async fn migrate(db: &Connection) -> anyhow::Result<()> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        (),
    )
    .await?;
    let current = version(db).await?;
    for (target, sql) in MIGRATIONS.iter().filter(|(target, _)| *target > current) {
        let statements = statements(sql);
        let mut batch: Vec<(&str, Vec<Value>)> = statements
            .iter()
            .map(|statement| (statement.as_str(), Vec::new()))
            .collect();
        batch.push((
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            vec![
                Value::Integer(*target),
                Value::Integer(crate::auth::now_ms()),
            ],
        ));
        db.transactional_batch(batch, TransactionBehavior::Immediate)
            .await
            .with_context(|| format!("migration {target} failed"))?;
        tracing::info!(version = target, "applied database migration");
    }
    Ok(())
}

/// Opens a connection with foreign keys enforced; the database ignores them unless
/// every connection asks.
///
/// # Errors
///
/// Returns an error if the connection cannot be opened.
pub async fn connect(database: &turso::Database) -> anyhow::Result<Connection> {
    let db = database.connect()?;
    db.execute("PRAGMA foreign_keys = ON", ()).await?;
    Ok(db)
}

async fn version(db: &Connection) -> anyhow::Result<i64> {
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
pub(crate) async fn open_test() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("panel.db");
    let db = turso::Builder::new_local(path.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let conn = connect(&db).await.unwrap();
    migrate(&conn).await.unwrap();
    (dir, conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_migration_file_has_statements_and_versions_increase() {
        for (index, (version, sql)) in MIGRATIONS.iter().enumerate() {
            assert_eq!(*version, i64::try_from(index).unwrap() + 1);
            assert!(!statements(sql).is_empty(), "migration {version} is empty");
        }
    }

    #[tokio::test]
    async fn migrate_is_idempotent_and_records_the_latest_version() {
        let (_dir, db) = open_test().await;
        migrate(&db).await.unwrap();
        migrate(&db).await.unwrap();
        let latest = MIGRATIONS.last().map(|(v, _)| *v).unwrap();
        assert_eq!(version(&db).await.unwrap(), latest);
    }

    #[tokio::test]
    async fn foreign_keys_and_checks_are_enforced() {
        let (_dir, db) = open_test().await;
        let orphan = db
            .execute(
                "INSERT INTO members (project_id, handle, role) VALUES ('nope', 'nobody', 'reader')",
                (),
            )
            .await;
        assert!(orphan.is_err(), "orphan member was accepted");
        db.execute_batch(
            "INSERT INTO accounts VALUES ('ana', 'hash', 0, 0);
             INSERT INTO projects VALUES ('demo', 'Demo', 0);
             INSERT INTO members VALUES ('demo', 'ana', 'owner');",
        )
        .await
        .unwrap();
        let bad_role = db
            .execute("UPDATE members SET role = 'king' WHERE handle = 'ana'", ())
            .await;
        assert!(bad_role.is_err(), "unknown role was accepted");
        db.execute("DELETE FROM projects WHERE id = 'demo'", ())
            .await
            .unwrap();
        let mut rows = db.query("SELECT count(*) FROM members", ()).await.unwrap();
        let left: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(left, 0, "deleting a project must remove its members");
    }

    #[tokio::test]
    async fn a_device_token_needs_a_project_and_no_person() {
        let (_dir, db) = open_test().await;
        db.execute_batch(
            "INSERT INTO accounts VALUES ('ana', 'hash', 0, 0);
             INSERT INTO projects VALUES ('demo', 'Demo', 0);",
        )
        .await
        .unwrap();
        let mixed = db
            .execute(
                "INSERT INTO tokens (digest, kind, handle, project_id, label, created_at)
                 VALUES ('d', 'device', 'ana', 'demo', 'ci', 0)",
                (),
            )
            .await;
        assert!(
            mixed.is_err(),
            "device token owned by a person was accepted"
        );
    }
}
