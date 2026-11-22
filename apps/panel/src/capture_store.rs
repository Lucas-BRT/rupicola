//! Captures and their media artifacts in `panel.db`, with file content in [`Blobs`].

use crate::{
    BoxError,
    blobs::{self, Blobs, Stored},
};
use rupicola_core::capture_manifest::Manifest;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use turso::{Connection, Value, transaction::TransactionBehavior};

pub struct NewCapture {
    pub id: String,
    pub manifest: Manifest,
    /// Capture root and run directory the capture was imported from, so URLs
    /// recorded before the import keep resolving.
    pub legacy: Option<(String, String)>,
    pub artifacts: Vec<NewArtifact>,
}

pub struct NewArtifact {
    pub role: &'static str,
    pub owner_id: String,
    pub path: String,
    pub stored: Stored,
    pub target_ms: Option<f64>,
    pub video_ms: Option<f64>,
}

/// Inserts a capture and all its artifacts in one transaction. An existing id is left
/// unchanged.
///
/// # Errors
///
/// Returns an error if serialization or the transaction fails.
pub async fn insert(db: &Connection, capture: &NewCapture) -> Result<(), BoxError> {
    let run = &capture.manifest;
    let (legacy_root, legacy_name) = match &capture.legacy {
        Some((root, name)) => (Value::Text(root.clone()), Value::Text(name.clone())),
        None => (Value::Null, Value::Null),
    };
    let mut batch: Vec<(&str, Vec<Value>)> = vec![(
        "INSERT INTO captures (id, project_id, project_label, scenario, revision, status, blocker,
           inspection, environment_json, findings_json, created_at_ms, legacy_root, legacy_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO NOTHING",
        vec![
            Value::Text(capture.id.clone()),
            Value::Text(run.project.id.clone()),
            Value::Text(run.project.label.clone()),
            Value::Text(run.scenario.clone()),
            optional(run.revision.as_deref()),
            Value::Text(run.status.clone()),
            optional(run.blocker.as_deref()),
            Value::Text(run.inspection.clone()),
            Value::Text(serde_json::to_string(&run.environment)?),
            Value::Text(serde_json::to_string(&run.findings)?),
            Value::Integer(run.created_at_ms),
            legacy_root,
            legacy_name,
        ],
    )];
    for artifact in &capture.artifacts {
        batch.push((
            "INSERT INTO artifacts (id, capture_id, project_id, role, owner_id, path, sha256, bytes,
               mime, target_ms, video_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO NOTHING",
            vec![
                Value::Text(digest(&[&capture.id, &artifact.path])),
                Value::Text(capture.id.clone()),
                Value::Text(run.project.id.clone()),
                Value::Text(artifact.role.to_owned()),
                Value::Text(artifact.owner_id.clone()),
                Value::Text(artifact.path.clone()),
                Value::Text(artifact.stored.sha256.clone()),
                Value::Integer(i64::try_from(artifact.stored.bytes)?),
                Value::Text(blobs::mime(&artifact.path).to_owned()),
                artifact.target_ms.map_or(Value::Null, Value::Real),
                artifact.video_ms.map_or(Value::Null, Value::Real),
            ],
        ));
    }
    db.transactional_batch(batch, TransactionBehavior::Immediate)
        .await?;
    Ok(())
}

async fn has_legacy(db: &Connection, root: &str, name: &str) -> Result<bool, BoxError> {
    let mut rows = db
        .query(
            "SELECT 1 FROM captures WHERE legacy_root = ?1 AND legacy_name = ?2",
            turso::params![root, name],
        )
        .await?;
    Ok(rows.next().await?.is_some())
}

pub struct ArtifactRef {
    pub project_id: String,
    pub sha256: String,
    pub mime: String,
}

/// Looks up an artifact by id.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn artifact(db: &Connection, id: &str) -> Result<Option<ArtifactRef>, BoxError> {
    let rows = db
        .query(
            "SELECT project_id, sha256, mime FROM artifacts WHERE id = ?1",
            turso::params![id],
        )
        .await?;
    first_artifact(rows).await
}

/// Looks up an artifact by the capture root, run directory and path it was imported
/// from, which is how URLs recorded before the import address it.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn legacy_artifact(
    db: &Connection,
    root: &str,
    name: &str,
    path: &str,
) -> Result<Option<ArtifactRef>, BoxError> {
    let rows = db
        .query(
            "SELECT a.project_id, a.sha256, a.mime FROM artifacts a
             JOIN captures c ON c.id = a.capture_id
             WHERE c.legacy_root = ?1 AND c.legacy_name = ?2 AND a.path = ?3",
            turso::params![root, name, path],
        )
        .await?;
    first_artifact(rows).await
}

async fn first_artifact(mut rows: turso::Rows) -> Result<Option<ArtifactRef>, BoxError> {
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    Ok(Some(ArtifactRef {
        project_id: row.get(0)?,
        sha256: row.get(1)?,
        mime: row.get(2)?,
    }))
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub captures: usize,
    pub missing_files: usize,
}

/// Imports every run directory under `roots` that is not in the database yet.
/// Source directories are left untouched.
///
/// # Errors
///
/// Returns an error if a root cannot be listed, a blob cannot be stored, or a
/// database write fails.
pub async fn import_roots(
    db: &Connection,
    blobs: &Blobs,
    roots: &[PathBuf],
) -> Result<ImportReport, BoxError> {
    let mut report = ImportReport::default();
    for root in roots {
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let root_key = root_key(root)?;
        for entry in entries {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if let Some(missing) = import_run(db, blobs, &root_key, &entry.path(), &name).await? {
                report.captures += 1;
                report.missing_files += missing;
            }
        }
    }
    Ok(report)
}

/// Key that identifies a capture root across restarts: its canonical path.
///
/// # Errors
///
/// Returns an error if the root cannot be resolved.
pub fn root_key(root: &Path) -> std::io::Result<String> {
    Ok(std::fs::canonicalize(root)?.to_string_lossy().into_owned())
}

/// Imports one run directory if it holds a valid manifest and is not imported yet.
/// Returns the number of declared files that were missing, or `None` when nothing
/// was imported.
///
/// # Errors
///
/// Returns an error if a blob cannot be stored or the database write fails.
pub async fn import_run(
    db: &Connection,
    blobs: &Blobs,
    root_key: &str,
    dir: &Path,
    name: &str,
) -> Result<Option<usize>, BoxError> {
    if !crate::safe_name(name) || has_legacy(db, root_key, name).await? {
        return Ok(None);
    }
    let Some(manifest) = crate::read_manifest(dir) else {
        return Ok(None);
    };
    let (artifacts, missing) = store_artifacts(blobs, dir, &manifest)?;
    insert(
        db,
        &NewCapture {
            id: digest(&[root_key, name]),
            manifest,
            legacy: Some((root_key.to_owned(), name.to_owned())),
            artifacts,
        },
    )
    .await?;
    Ok(Some(missing))
}

fn store_artifacts(
    blobs: &Blobs,
    dir: &Path,
    manifest: &Manifest,
) -> Result<(Vec<NewArtifact>, usize), BoxError> {
    /// Role, owner id, path, frame target and frame video timestamps.
    type Wanted<'a> = (&'static str, &'a str, &'a str, Option<f64>, Option<f64>);
    let mut wanted: Vec<Wanted> = Vec::new();
    for checkpoint in &manifest.checkpoints {
        wanted.push(("checkpoint", &checkpoint.id, &checkpoint.path, None, None));
    }
    for clip in &manifest.clips {
        for (role, path) in [
            ("clip-video", clip.video_path.as_deref()),
            ("clip-sheet", clip.sheet_path.as_deref()),
            ("clip-analysis", clip.analysis_path.as_deref()),
        ] {
            if let Some(path) = path {
                wanted.push((role, &clip.id, path, None, None));
            }
        }
        for frame in &clip.frames {
            wanted.push((
                "clip-frame",
                &clip.id,
                &frame.path,
                Some(frame.target_ms),
                frame.video_ms,
            ));
        }
    }
    let mut artifacts = Vec::new();
    let mut missing = 0;
    for (role, owner_id, path, target_ms, video_ms) in wanted {
        if artifacts.iter().any(|a: &NewArtifact| a.path == path) {
            continue;
        }
        let Ok(source) = crate::media::resolve_checked_path(dir.to_path_buf(), path) else {
            missing += 1;
            continue;
        };
        artifacts.push(NewArtifact {
            role,
            owner_id: owner_id.to_owned(),
            path: path.to_owned(),
            stored: blobs.put_file(&source)?,
            target_ms,
            video_ms,
        });
    }
    Ok((artifacts, missing))
}

fn optional(text: Option<&str>) -> Value {
    text.map_or(Value::Null, |text| Value::Text(text.to_owned()))
}

/// Stable 128-bit hex id derived from `parts`.
fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())[..32].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_run(root: &Path, name: &str) {
        let run = root.join(name);
        fs::create_dir_all(run.join("checkpoints")).unwrap();
        fs::write(run.join("checkpoints/home.png"), b"png-bytes").unwrap();
        fs::write(
            run.join("manifest.json"),
            r#"{"version":1,"project":{"id":"p1","label":"Demo"},"scenario":"home",
                "revision":null,"created_at_ms":7,"status":"complete","blocker":null,
                "inspection":"passed","environment":{"color_scheme":"dark","reduced_motion":false,
                "width":1280,"height":720,"renderer":"chromium"},
                "checkpoints":[{"id":"home","path":"checkpoints/home.png"},
                               {"id":"gone","path":"checkpoints/gone.png"}],
                "clips":[]}"#,
        )
        .unwrap();
    }

    #[tokio::test]
    async fn imports_runs_once_and_counts_missing_files() {
        let (dir, db) = crate::db::test_db().await;
        crate::db::migrate(&db).await.unwrap();
        let blobs = Blobs::open(dir.path().join("blobs")).unwrap();
        let root = dir.path().join("captures");
        write_run(&root, "run-1");

        let roots = std::slice::from_ref(&root);
        let first = import_roots(&db, &blobs, roots).await.unwrap();
        let second = import_roots(&db, &blobs, roots).await.unwrap();

        assert_eq!(
            first,
            ImportReport {
                captures: 1,
                missing_files: 1
            }
        );
        assert_eq!(second, ImportReport::default());
        let mut rows = db
            .query(
                "SELECT role, owner_id, path, mime, bytes FROM artifacts",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.get::<String>(0).unwrap(), "checkpoint");
        assert_eq!(row.get::<String>(2).unwrap(), "checkpoints/home.png");
        assert_eq!(row.get::<String>(3).unwrap(), "image/png");
        assert_eq!(row.get::<i64>(4).unwrap(), 9);
        assert!(rows.next().await.unwrap().is_none());
        assert!(root.join("run-1/checkpoints/home.png").is_file());
    }
}
