//! SQLite index for Problems, Fixes, Proposals, and AI Provider Stage Config.
//! Keeps disk JSON files synchronized with `panel.db` so queries do not scan
//! every JSON file per request and malformed records emit explicit diagnostics.

use rupicola_core::{domain, fix_record, problem_record, proposal_record};
use std::{fs, path::Path};
use turso::Connection;

const SCHEMA_SQL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS problems_v1 (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        status TEXT NOT NULL,
        severity TEXT NOT NULL,
        linked_fix_id TEXT,
        updated_at_ms INTEGER NOT NULL,
        payload_json TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_problems_project_status ON problems_v1(project_id, status)",
    "CREATE TABLE IF NOT EXISTS fixes_v1 (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        status TEXT NOT NULL,
        scope TEXT,
        updated_at_ms INTEGER NOT NULL,
        payload_json TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_fixes_project_status ON fixes_v1(project_id, status)",
    "CREATE TABLE IF NOT EXISTS proposals_v1 (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        status TEXT NOT NULL,
        updated_at_ms INTEGER NOT NULL,
        payload_json TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_proposals_project_status ON proposals_v1(project_id, status)",
    "CREATE TABLE IF NOT EXISTS ai_providers_v1 (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL DEFAULT '',
        stage TEXT NOT NULL,
        provider TEXT NOT NULL,
        model TEXT NOT NULL,
        autonomy TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1
    )",
    "CREATE TABLE IF NOT EXISTS captures_v1 (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        backend TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        payload_json TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_captures_project_status ON captures_v1(project_id, status)",
    "CREATE TABLE IF NOT EXISTS artifacts_v1 (
        id TEXT PRIMARY KEY,
        capture_id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        path TEXT NOT NULL,
        annotated INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE INDEX IF NOT EXISTS idx_artifacts_capture ON artifacts_v1(capture_id)",
];

/// Initializes queue and record index tables in `panel.db`.
///
/// # Errors
///
/// Returns an error if any DDL statement fails.
pub async fn init(db: &Connection) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for stmt in SCHEMA_SQL {
        db.execute(stmt, ()).await?;
    }
    Ok(())
}

/// Inserts or updates a [`problem_record::Problem`] in `problems_v1`.
///
/// # Errors
///
/// Returns an error if serialization or SQLite execution fails.
pub async fn upsert_problem(
    db: &Connection,
    item: &problem_record::Problem,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let payload = serde_json::to_string(item)?;
    let updated = i64::try_from(item.updated_at_ms).unwrap_or(i64::MAX);
    db.execute(
        "INSERT INTO problems_v1 (id, project_id, status, severity, linked_fix_id, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
           project_id = excluded.project_id,
           status = excluded.status,
           severity = excluded.severity,
           linked_fix_id = excluded.linked_fix_id,
           updated_at_ms = excluded.updated_at_ms,
           payload_json = excluded.payload_json",
        turso::params![
            item.id.as_str(),
            item.project_id.as_str(),
            item.status.slug(),
            item.severity.slug(),
            item.linked_fix_id.as_deref(),
            updated,
            payload
        ],
    )
    .await?;
    Ok(())
}

/// Inserts or updates a [`fix_record::FixRecord`] in `fixes_v1`.
///
/// # Errors
///
/// Returns an error if serialization or SQLite execution fails.
pub async fn upsert_fix(
    db: &Connection,
    item: &fix_record::FixRecord,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let payload = serde_json::to_string(item)?;
    let updated = i64::try_from(item.updated_at_ms).unwrap_or(i64::MAX);
    let scope: Option<&str> = None;
    db.execute(
        "INSERT INTO fixes_v1 (id, project_id, status, scope, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
           project_id = excluded.project_id,
           status = excluded.status,
           scope = excluded.scope,
           updated_at_ms = excluded.updated_at_ms,
           payload_json = excluded.payload_json",
        turso::params![
            item.id.as_str(),
            item.project_id.as_str(),
            item.status.slug(),
            scope,
            updated,
            payload
        ],
    )
    .await?;
    Ok(())
}

/// Inserts or updates a [`proposal_record::Proposal`] in `proposals_v1`.
///
/// # Errors
///
/// Returns an error if serialization or SQLite execution fails.
pub async fn upsert_proposal(
    db: &Connection,
    project_id: &str,
    item: &proposal_record::Proposal,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let payload = serde_json::to_string(item)?;
    let updated = i64::try_from(item.timestamp_ms).unwrap_or(i64::MAX);
    db.execute(
        "INSERT INTO proposals_v1 (id, project_id, status, updated_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
           project_id = excluded.project_id,
           status = excluded.status,
           updated_at_ms = excluded.updated_at_ms,
           payload_json = excluded.payload_json",
        turso::params![
            item.id.as_str(),
            project_id,
            item.status.slug(),
            updated,
            payload
        ],
    )
    .await?;
    Ok(())
}

/// Inserts or updates a [`domain::CaptureRecord`] and registers all its media
/// files in `artifacts_v1` so media lookup no longer depends solely on `manifest.json`.
///
/// # Errors
///
/// Returns an error if serialization or SQLite execution fails.
#[allow(
    dead_code,
    reason = "wired incrementally as Phase 2 handlers migrate to DomainService"
)]
pub async fn upsert_capture_with_artifacts(
    db: &Connection,
    record: &domain::CaptureRecord,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let payload = serde_json::to_string(record)?;
    let created = i64::try_from(record.created_at_ms).unwrap_or(i64::MAX);
    db.execute(
        "INSERT INTO captures_v1 (id, project_id, backend, status, created_at_ms, payload_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
           project_id = excluded.project_id,
           backend = excluded.backend,
           status = excluded.status,
           created_at_ms = excluded.created_at_ms,
           payload_json = excluded.payload_json",
        turso::params![
            record.id.as_str(),
            record.project_id.as_str(),
            record.backend.as_str(),
            record.status.slug(),
            created,
            payload
        ],
    )
    .await?;

    let mut entries: Vec<(&str, String, i64)> = vec![(
        "image",
        record.output.screenshot_png.to_string_lossy().into_owned(),
        0,
    )];
    if let Some(ref p) = record.output.annotated_png {
        entries.push(("image", p.to_string_lossy().into_owned(), 1));
    }
    if let Some(ref p) = record.output.video_mp4 {
        entries.push(("video", p.to_string_lossy().into_owned(), 0));
    }
    if let Some(ref p) = record.output.annotated_video_mp4 {
        entries.push(("video", p.to_string_lossy().into_owned(), 1));
    }
    if let Some(ref p) = record.output.video_gif {
        entries.push(("gif", p.to_string_lossy().into_owned(), 0));
    }
    if let Some(ref p) = record.output.annotated_video_gif {
        entries.push(("gif", p.to_string_lossy().into_owned(), 1));
    }

    for (idx, (kind, path, annotated)) in entries.into_iter().enumerate() {
        let art_id = format!("{}:{idx}", record.id);
        db.execute(
            "INSERT INTO artifacts_v1 (id, capture_id, project_id, kind, path, annotated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, path = excluded.path, annotated = excluded.annotated",
            turso::params![
                art_id.as_str(),
                record.id.as_str(),
                record.project_id.as_str(),
                kind,
                path.as_str(),
                annotated
            ],
        )
        .await?;
    }
    Ok(())
}

/// Checks whether `relative_path` is registered in `artifacts_v1` for `project_id`.
pub async fn has_indexed_artifact(db: &Connection, project_id: &str, relative_path: &str) -> bool {
    let Ok(mut rows) = db
        .query(
            "SELECT 1 FROM artifacts_v1 WHERE project_id = ?1 AND path = ?2 LIMIT 1",
            turso::params![project_id, relative_path],
        )
        .await
    else {
        return false;
    };
    matches!(rows.next().await, Ok(Some(_)))
}

/// Scans `data_root/{problems,fixes,proposes}` on startup, indexing valid JSON
/// records into SQLite and logging any malformed JSON instead of failing silently.
///
/// # Errors
///
/// Returns an error if database writes fail.
pub async fn sync_from_disk(
    db: &Connection,
    data_root: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for project in project_dirs(&data_root.join(problem_record::DIRECTORY)) {
        for path in json_files(&data_root.join(problem_record::DIRECTORY).join(&project)) {
            match fs::read(&path).map_err(|e| e.to_string()).and_then(|b| {
                serde_json::from_slice::<problem_record::Problem>(&b).map_err(|e| e.to_string())
            }) {
                Ok(item) => upsert_problem(db, &item).await?,
                Err(err) => eprintln!("rupicola-panel: invalid problem {}: {err}", path.display()),
            }
        }
    }
    for project in project_dirs(&data_root.join(fix_record::DIRECTORY)) {
        for path in json_files(&data_root.join(fix_record::DIRECTORY).join(&project)) {
            match fs::read(&path).map_err(|e| e.to_string()).and_then(|b| {
                serde_json::from_slice::<fix_record::FixRecord>(&b).map_err(|e| e.to_string())
            }) {
                Ok(item) => upsert_fix(db, &item).await?,
                Err(err) => eprintln!("rupicola-panel: invalid fix {}: {err}", path.display()),
            }
        }
    }
    for project in project_dirs(&data_root.join(proposal_record::DIRECTORY)) {
        for path in json_files(&data_root.join(proposal_record::DIRECTORY).join(&project)) {
            match fs::read(&path).map_err(|e| e.to_string()).and_then(|b| {
                serde_json::from_slice::<proposal_record::Proposal>(&b).map_err(|e| e.to_string())
            }) {
                Ok(item) => upsert_proposal(db, &project, &item).await?,
                Err(err) => eprintln!("rupicola-panel: invalid proposal {}: {err}", path.display()),
            }
        }
    }
    Ok(())
}

fn project_dirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

fn json_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_and_upsert_problem_works() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = turso::Builder::new_local(db_path.to_str().unwrap())
            .build()
            .await
            .unwrap();
        let conn = db.connect().unwrap();
        init(&conn).await.unwrap();

        let p = problem_record::Problem {
            id: "p1".into(),
            project_id: "proj1".into(),
            title: "Test".into(),
            status: problem_record::ProblemStatus::Open,
            severity: problem_record::Severity::Major,
            origin: problem_record::ProblemOrigin::UserReported,
            route: "/".into(),
            viewport: "1440x900".into(),
            theme: "light".into(),
            diagnosis: "Diag".into(),
            proposed_solution: "Fix".into(),
            violated_rules: vec![],
            evidence_assets: vec![],
            comments: vec![],
            linked_fix_id: None,
            execution: None,
            author: "dev".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        upsert_problem(&conn, &p).await.unwrap();
        let mut rows = conn
            .query(
                "SELECT id, status FROM problems_v1 WHERE project_id = 'proj1'",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.get::<String>(0).unwrap(), "p1");
        assert_eq!(row.get::<String>(1).unwrap(), "open");

        let cap = domain::CaptureRecord {
            id: "cap1".into(),
            project_id: "proj1".into(),
            backend: "web-playwright".into(),
            target: "http://127.0.0.1:4732".into(),
            status: domain::CaptureQueueStatus::PendingAnalysis,
            output: domain::CaptureOutput {
                screenshot_png: std::path::PathBuf::from("checkpoints/preview.png"),
                annotated_png: Some(std::path::PathBuf::from(
                    "checkpoints/preview-annotated.png",
                )),
                video_mp4: Some(std::path::PathBuf::from("clips/walkthrough.mp4")),
                annotated_video_mp4: Some(std::path::PathBuf::from(
                    "clips/walkthrough-annotated.mp4",
                )),
                video_gif: Some(std::path::PathBuf::from("clips/walkthrough.gif")),
                annotated_video_gif: Some(std::path::PathBuf::from(
                    "clips/walkthrough-annotated.gif",
                )),
                snapshot: None,
            },
            created_at_ms: 1,
        };
        upsert_capture_with_artifacts(&conn, &cap).await.unwrap();
        assert!(has_indexed_artifact(&conn, "proj1", "clips/walkthrough-annotated.gif").await);
        assert!(
            !has_indexed_artifact(&conn, "other-proj", "clips/walkthrough-annotated.gif").await
        );
    }
}
