use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use http::HeaderMap;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Write,
    process::{Command, Stdio},
};
use topcoat::router::{content::Json, to_bytes};

const MAX_MEDIA: usize = 40 * 1024 * 1024;
const MAX_ENVELOPE: u64 = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub struct Bundle {
    manifest: serde_json::Value,
    files: BTreeMap<String, String>,
}

fn paths(manifest: &Manifest) -> BTreeSet<&str> {
    manifest
        .checkpoints
        .iter()
        .map(|item| item.path.as_str())
        .chain(manifest.clips.iter().flat_map(|clip| {
            [
                clip.video_path.as_deref(),
                clip.sheet_path.as_deref(),
                clip.analysis_path.as_deref(),
            ]
            .into_iter()
            .flatten()
            .chain(clip.frames.iter().map(|frame| frame.path.as_str()))
        }))
        .collect()
}

pub fn valid_file(path: &str) -> bool {
    path.len() <= 180
        && path.matches('/').count() <= 4
        && matches!(
            FilePath::new(path).extension().and_then(|ext| ext.to_str()),
            Some("png" | "mp4" | "webm" | "json")
        )
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
}

fn object_key(project: &str, bundle: &Bundle) -> Result<String, serde_json::Error> {
    let mut hasher = Sha256::new();
    hasher.update(project.as_bytes());
    serde_json::to_writer(&mut hasher, bundle)?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub async fn publish(
    panel: Arc<Panel>,
    project: String,
    headers: HeaderMap,
    body: Body,
) -> PageResult<(StatusCode, Json<serde_json::Value>)> {
    if !safe_name(&project) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let uploads = panel.uploads.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let digest = auth::digest(bearer);
    {
        let db = panel.index.lock().await;
        if device_label(&db, &project, &digest).await?.is_none()
            && machine_token_owner(&db, &digest).await?.is_none()
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    let permit = panel
        .upload_work
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;

    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        to_bytes(body, MAX_ENVELOPE as usize),
    )
    .await
    .map_err(|_| StatusCode::REQUEST_TIMEOUT)?
    .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    let bundle: Bundle =
        serde_json::from_slice(&bytes).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    drop(bytes);
    let project_copy = project.clone();
    let uploads = uploads.clone();
    let (stage, target, key, project_label, permit) = tokio::task::spawn_blocking(move || {
        let (stage, target, key, project_label) = stage_bundle(&uploads, &project_copy, bundle)?;
        Ok::<_, StatusCode>((stage, target, key, project_label, permit))
    })
    .await
    .map_err(db_error)??;
    let _permit = permit;
    let db = panel.index.lock().await;
    let device = device_label(&db, &project, &digest).await?;
    let machine = if device.is_none() {
        machine_token_owner(&db, &digest).await?
    } else {
        None
    };
    if device.is_none() && machine.is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if target.is_dir() {
        return Ok((StatusCode::OK, Json(serde_json::json!({ "run": key }))));
    }
    fs::rename(stage.path(), target).map_err(db_error)?;
    if let Some(label) = device {
        db.transactional_batch(
            [
                (
                    "UPDATE devices SET last_seen = ? WHERE digest = ?",
                    Vec::from(turso::params![auth::now(), digest]),
                ),
                (
                    "INSERT INTO audit(project, actor, event, occurred) VALUES (?, ?, 'Capture published', ?)",
                    Vec::from(turso::params![project, label, auth::now()]),
                ),
            ],
            TransactionBehavior::Immediate,
        )
        .await
        .map_err(db_error)?;
    } else if let Some((owner, label)) = machine {
        let proj_label = if project_label.trim().is_empty() {
            project.clone()
        } else {
            project_label
        };
        db.transactional_batch(
            [
                (
                    "INSERT OR IGNORE INTO projects (id, label) VALUES (?, ?)",
                    Vec::from(turso::params![project.clone(), proj_label]),
                ),
                (
                    "INSERT OR IGNORE INTO members (project, handle, role) VALUES (?, ?, 'owner')",
                    Vec::from(turso::params![project.clone(), owner]),
                ),
                (
                    "UPDATE machine_tokens SET last_seen = ? WHERE digest = ?",
                    Vec::from(turso::params![auth::now(), digest]),
                ),
                (
                    "INSERT INTO audit(project, actor, event, occurred) VALUES (?, ?, 'Capture published', ?)",
                    Vec::from(turso::params![project, label, auth::now()]),
                ),
            ],
            TransactionBehavior::Immediate,
        )
        .await
        .map_err(db_error)?;
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "run": key }))))
}

async fn machine_token_owner(
    db: &Connection,
    digest: &str,
) -> PageResult<Option<(String, String)>> {
    let mut rows = db
        .query(
            "SELECT owner, label FROM machine_tokens WHERE digest = ?",
            [digest],
        )
        .await
        .map_err(db_error)?;
    rows.next()
        .await
        .map_err(db_error)?
        .map(|row| Ok::<_, turso::Error>((row.get(0)?, row.get(1)?)))
        .transpose()
        .map_err(db_error)
}

async fn device_label(db: &Connection, project: &str, digest: &str) -> PageResult<Option<String>> {
    let mut rows = db
        .query(
            "SELECT label FROM devices WHERE project = ? AND digest = ?",
            [project, digest],
        )
        .await
        .map_err(db_error)?;
    rows.next()
        .await
        .map_err(db_error)?
        .map(|row| row.get(0))
        .transpose()
        .map_err(db_error)
}

fn stage_bundle(
    uploads: &FilePath,
    project: &str,
    mut bundle: Bundle,
) -> PageResult<(tempfile::TempDir, PathBuf, String, String)> {
    let manifest: Manifest = serde_json::from_value(bundle.manifest.clone())
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let project_label = manifest.project.label.clone();
    if manifest.version != 1
        || manifest.project.id != project
        || !matches!(manifest.status.as_str(), "complete" | "blocked")
        || manifest.scenario.is_empty()
        || manifest.scenario.len() > 512
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    bundle.manifest["inspection"] = serde_json::json!("pending");
    bundle.manifest["findings"] = serde_json::json!([]);
    let metadata = serde_json::to_vec(&bundle.manifest).map_err(db_error)?;
    let files = paths(&manifest);
    if metadata.len() > 1024 * 1024
        || files.len() > 256
        || files.len() != bundle.files.len()
        || files
            .iter()
            .any(|file| !valid_file(file) || !bundle.files.contains_key(*file))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let key = object_key(project, &bundle).map_err(db_error)?;
    let target = uploads.join(&key);
    let stage = tempfile::Builder::new()
        .prefix(".incoming-")
        .tempdir_in(uploads)
        .map_err(db_error)?;
    let mut total = 0;
    for (file, encoded) in bundle.files {
        let decoded = STANDARD
            .decode(encoded)
            .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
        total += decoded.len();
        if total > MAX_MEDIA {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        let path = stage.path().join(file);
        fs::create_dir_all(path.parent().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?)
            .map_err(db_error)?;
        fs::File::options()
            .create_new(true)
            .write(true)
            .open(path)
            .and_then(|mut file| file.write_all(&decoded))
            .map_err(db_error)?;
    }
    fs::write(stage.path().join("manifest.json"), metadata).map_err(db_error)?;
    Ok((stage, target, key, project_label))
}

pub fn send(run: &FilePath, server: &str) -> Result<(), BoxError> {
    let origin = url::Url::parse(server)?;
    let local = matches!(origin.host_str(), Some("localhost" | "127.0.0.1"));
    if !(origin.scheme() == "https" || local && origin.scheme() == "http")
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return Err("provide a plain HTTPS origin (HTTP is allowed for loopback)".into());
    }
    let token = std::env::var("RUPICOLA_PUBLISH_TOKEN")?;
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("RUPICOLA_PUBLISH_TOKEN must contain a valid publishing credential".into());
    }
    let manifest = read_manifest(run).ok_or("invalid capture manifest")?;
    if !safe_name(&manifest.project.id) {
        return Err("invalid project ID".into());
    }
    let mut files = BTreeMap::new();
    let mut total = 0;
    let root = fs::canonicalize(run)?;
    for file in paths(&manifest) {
        if !valid_file(file) {
            return Err("invalid artifact path".into());
        }
        let path = fs::canonicalize(root.join(file))?;
        if !path.starts_with(&root) || !path.is_file() {
            return Err("artifact outside capture directory".into());
        }
        let size = fs::metadata(&path)?.len();
        if size > (MAX_MEDIA - total) as u64 {
            return Err("capture exceeds 40 MiB of media".into());
        }
        let bytes = fs::read(path)?;
        total += bytes.len();
        if total > MAX_MEDIA {
            return Err("capture exceeds 40 MiB of media".into());
        }
        files.insert(file.to_owned(), STANDARD.encode(bytes));
    }
    let bundle = Bundle {
        manifest: serde_json::from_slice(&fs::read(root.join("manifest.json"))?)?,
        files,
    };
    let mut body = tempfile::NamedTempFile::new()?;
    serde_json::to_writer(body.as_file_mut(), &bundle)?;
    body.as_file_mut().flush()?;
    let url = format!(
        "{}/api/publish/{}",
        origin.origin().ascii_serialization(),
        manifest.project.id
    );
    let mut child = Command::new("curl")
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--fail-with-body",
            "--max-time",
            "120",
            "--header",
            "@-",
            "--header",
            "Content-Type: application/json",
            "--data-binary",
        ])
        .arg(format!("@{}", body.path().display()))
        .arg(url)
        .stdin(Stdio::piped())
        .env_remove("RUPICOLA_PUBLISH_TOKEN")
        .spawn()?;
    let write = writeln!(
        child.stdin.take().ok_or("curl input unavailable")?,
        "Authorization: Bearer {token}"
    );
    let status = child.wait()?;
    write?;
    if !status.success() {
        return Err(format!("upload failed ({status})").into());
    }
    Ok(())
}
