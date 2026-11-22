use super::{
    Account, Arc, Body, Component, FilePath, Manifest, PageResult, Panel, Request, Response,
    ServeFile, StatusCode, auth, fs, read_manifest, records_index, run_dir,
};

pub async fn file(
    account: Account,
    panel: Arc<Panel>,
    (root, name, file): (usize, String, String),
    request: Request,
) -> PageResult<Response> {
    let dir = run_dir(&panel, root, &name).ok_or(StatusCode::NOT_FOUND)?;
    let manifest = read_manifest(&dir).ok_or(StatusCode::NOT_FOUND)?;
    auth::role(&panel, &account, &manifest.project.id).await?;
    let source = match resolve_artifact(dir.clone(), &manifest, &file) {
        Ok(path) => path,
        Err(StatusCode::NOT_FOUND)
            if records_index::has_indexed_artifact(
                &*panel.index.lock().await,
                &manifest.project.id,
                &file,
            )
            .await =>
        {
            resolve_checked_path(dir, &file)?
        }
        Err(err) => return Err(err),
    };
    Ok(ServeFile::new(source)
        .try_call(request)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Body::new))
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

fn resolve_checked_path(dir: std::path::PathBuf, file: &str) -> PageResult<std::path::PathBuf> {
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
