use super::{
    Account, Arc, Body, Component, FilePath, Manifest, PageResult, Panel, Request, Response,
    ServeFile, StatusCode, auth, capture_store, db_error, fs, header, run_dir,
};
use capture_store::ArtifactRef;

/// Serves an artifact by id: `/media/a/{id}`.
pub async fn by_id(
    account: Account,
    panel: Arc<Panel>,
    id: String,
    request: Request,
) -> PageResult<Response> {
    let found = capture_store::artifact(&*panel.index.lock().await, &id)
        .await
        .map_err(db_error)?;
    serve(&panel, &account, found, request).await
}

/// Serves an artifact by the run path recorded before captures moved into the
/// database: `/media/{root}/{name}/{file}`. A run written after startup is imported
/// on first request.
pub async fn file(
    account: Account,
    panel: Arc<Panel>,
    (root, name, file): (usize, String, String),
    request: Request,
) -> PageResult<Response> {
    let dir = run_dir(&panel, root, &name).ok_or(StatusCode::NOT_FOUND)?;
    let root_key = &panel.root_keys[root];
    let found = {
        let db = panel.index.lock().await;
        let mut found = capture_store::legacy_artifact(&db, root_key, &name, &file)
            .await
            .map_err(db_error)?;
        if found.is_none() {
            capture_store::import_run(&db, &panel.blobs, root_key, &dir, &name)
                .await
                .map_err(db_error)?;
            found = capture_store::legacy_artifact(&db, root_key, &name, &file)
                .await
                .map_err(db_error)?;
        }
        found
    };
    serve(&panel, &account, found, request).await
}

async fn serve(
    panel: &Panel,
    account: &Account,
    found: Option<ArtifactRef>,
    request: Request,
) -> PageResult<Response> {
    let artifact = found.ok_or(StatusCode::NOT_FOUND)?;
    auth::role(panel, account, &artifact.project_id).await?;
    let source = panel
        .blobs
        .path(&artifact.sha256)
        .ok_or(StatusCode::NOT_FOUND)?;
    let mut response = ServeFile::new(source)
        .try_call(request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Body::new);
    // Blobs have no extension, so the stored media type replaces the guessed one.
    let mime = header::HeaderValue::from_str(&artifact.mime).map_err(db_error)?;
    response.headers_mut().insert(header::CONTENT_TYPE, mime);
    Ok(response)
}

pub fn resolve_artifact(
    dir: std::path::PathBuf,
    manifest: &Manifest,
    file: &str,
) -> PageResult<std::path::PathBuf> {
    if !declared(manifest, file) {
        return Err(StatusCode::NOT_FOUND);
    }
    resolve_checked_path(dir, file)
}

pub(crate) fn resolve_checked_path(
    dir: std::path::PathBuf,
    file: &str,
) -> PageResult<std::path::PathBuf> {
    let path = FilePath::new(file);
    if !matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("png" | "mp4" | "webm" | "gif" | "json")
    ) {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut source = dir;
    for part in path.components() {
        let Component::Normal(part) = part else {
            return Err(StatusCode::NOT_FOUND);
        };
        source.push(part);
        if fs::symlink_metadata(&source)
            .map_err(|_| StatusCode::NOT_FOUND)?
            .is_symlink()
        {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    if !source.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(source)
}

pub fn declared(manifest: &Manifest, file: &str) -> bool {
    manifest.checkpoints.iter().any(|item| item.path == file)
        || manifest.clips.iter().any(|clip| {
            [
                clip.video_path.as_deref(),
                clip.sheet_path.as_deref(),
                clip.analysis_path.as_deref(),
            ]
            .contains(&Some(file))
                || clip.frames.iter().any(|frame| frame.path == file)
        })
}
