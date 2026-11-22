//! Versioned schema migrations for `panel.db`, read from the numbered `.sql` files
//! in `apps/backend/migrations/`.
//!
//! Each file runs once, inside one transaction, and is recorded in
//! `schema_migrations`. Never edit a file that has shipped: add a new one.
//! Versions 1 and 2 match what the web panel created before the backend took the
//! database over, so the backend opens an existing `panel.db` unchanged.

use anyhow::Context;
use turso::{Connection, Value, transaction::TransactionBehavior};

/// Migration files in `apps/backend/migrations/`, embedded at build time.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_captures.sql")),
    (2, include_str!("../migrations/0002_panel_baseline.sql")),
    (3, include_str!("../migrations/0003_tokens.sql")),
];

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
            vec![Value::Integer(*target), Value::Integer(crate::auth::now())],
        ));
        db.transactional_batch(batch, TransactionBehavior::Immediate)
            .await
            .with_context(|| format!("migration {target} failed"))?;
        tracing::info!(version = target, "applied database migration");
    }
    Ok(())
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
    let conn = db.connect().unwrap();
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
    async fn opens_a_database_the_panel_created_and_keeps_its_credentials() {
        let (_dir, db) = open_test().await;
        // What an existing panel.db looks like: panel tables plus migration 1.
        db.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);
             CREATE TABLE sessions (id TEXT PRIMARY KEY, handle TEXT NOT NULL, csrf TEXT NOT NULL, expires INTEGER NOT NULL);
             CREATE TABLE machine_tokens (digest TEXT PRIMARY KEY, owner TEXT NOT NULL, label TEXT NOT NULL, created INTEGER NOT NULL, last_seen INTEGER);
             CREATE TABLE devices (digest TEXT PRIMARY KEY, project TEXT NOT NULL, label TEXT NOT NULL, created INTEGER NOT NULL, last_seen INTEGER);
             INSERT INTO sessions VALUES ('s-digest', 'ana', 'csrf', 99);
             INSERT INTO machine_tokens VALUES ('m-digest', 'ana', 'laptop', 5, NULL);
             INSERT INTO devices VALUES ('d-digest', 'demo', 'ci', 6, 7);",
        )
        .await
        .unwrap();
        for statement in statements(MIGRATIONS[0].1) {
            db.execute(&statement, ()).await.unwrap();
        }
        db.execute("INSERT INTO schema_migrations VALUES (1, 0)", ())
            .await
            .unwrap();

        migrate(&db).await.unwrap();

        let mut rows = db
            .query(
                "SELECT digest || ':' || kind || ':' || COALESCE(handle, project) FROM tokens ORDER BY digest",
                (),
            )
            .await
            .unwrap();
        let mut found = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            found.push(row.get::<String>(0).unwrap());
        }
        assert_eq!(
            found,
            [
                "d-digest:device:demo",
                "m-digest:machine:ana",
                "s-digest:session:ana"
            ]
        );
    }
}
