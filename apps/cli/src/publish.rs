//! Publishing a capture directory (a `manifest.json` and its media files) to the backend.

use crate::session::Remote;
use anyhow::{Context, Result, bail};
use rupicola_api::{
    BlobRef, CaptureStatus, Environment, NewCapture, NewCheckpoint, NewClip, NewFrame,
};
use rupicola_client::Api;
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

/// Largest manifest read, in bytes.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Manifest format this command understands.
const MANIFEST_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";

#[derive(Deserialize)]
struct ManifestProject {
    id: String,
}

#[derive(Deserialize)]
struct ManifestEnvironment {
    #[serde(default)]
    color_scheme: String,
    #[serde(default)]
    reduced_motion: bool,
    width: Option<u32>,
    height: Option<u32>,
    #[serde(default)]
    renderer: String,
}

#[derive(Deserialize)]
struct ManifestCheckpoint {
    id: String,
    path: String,
}

#[derive(Deserialize)]
struct ManifestFrame {
    path: String,
    target_ms: f64,
    video_ms: Option<f64>,
}

#[derive(Deserialize)]
struct ManifestClip {
    id: String,
    frames: Vec<ManifestFrame>,
    analysis_path: Option<String>,
    video_path: Option<String>,
    sheet_path: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    version: u32,
    project: ManifestProject,
    scenario: String,
    revision: Option<String>,
    status: String,
    blocker: Option<String>,
    environment: ManifestEnvironment,
    checkpoints: Vec<ManifestCheckpoint>,
    clips: Vec<ManifestClip>,
}

/// A capture directory on disk.
pub struct Bundle {
    root: PathBuf,
    manifest: Manifest,
}

impl Bundle {
    /// Reads `dir`'s manifest.
    ///
    /// # Errors
    ///
    /// Fails when the manifest is missing, too large, invalid or of another version.
    pub async fn open(dir: &Path) -> Result<Self> {
        let root = tokio::fs::canonicalize(dir)
            .await
            .with_context(|| format!("open {}", dir.display()))?;
        let path = root.join(MANIFEST_FILE);
        if tokio::fs::metadata(&path).await?.len() > MAX_MANIFEST_BYTES {
            bail!("{MANIFEST_FILE} is larger than {MAX_MANIFEST_BYTES} bytes");
        }
        let manifest: Manifest = serde_json::from_slice(&tokio::fs::read(&path).await?)
            .with_context(|| format!("read {}", path.display()))?;
        if manifest.version != MANIFEST_VERSION {
            bail!("expected a version {MANIFEST_VERSION} capture manifest");
        }
        Ok(Self { root, manifest })
    }

    /// Project the capture was taken for.
    pub fn project(&self) -> &str {
        &self.manifest.project.id
    }

    /// Reads the file at `relative` (inside the bundle only) and uploads it.
    async fn upload(&self, remote: &Remote, relative: &str) -> Result<BlobRef> {
        let relative_path = Path::new(relative);
        if relative_path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            bail!("{relative} is not a path inside the capture");
        }
        let file = tokio::fs::canonicalize(self.root.join(relative_path)).await?;
        if !file.starts_with(&self.root) || !tokio::fs::metadata(&file).await?.is_file() {
            bail!("{relative} is not a regular file of the capture");
        }
        let sha256 = remote
            .api
            .put_blob(&remote.token, &tokio::fs::read(&file).await?)
            .await?;
        Ok(BlobRef {
            path: relative.to_owned(),
            sha256,
        })
    }

    async fn upload_optional(
        &self,
        remote: &Remote,
        relative: Option<&String>,
    ) -> Result<Option<BlobRef>> {
        match relative {
            Some(path) => self.upload(remote, path).await.map(Some),
            None => Ok(None),
        }
    }

    fn environment(environment: &ManifestEnvironment) -> Environment {
        Environment {
            width: environment.width,
            height: environment.height,
            color_scheme: Some(environment.color_scheme.clone())
                .filter(|scheme| !scheme.is_empty()),
            reduced_motion: environment.reduced_motion,
            renderer: Some(environment.renderer.clone()).filter(|renderer| !renderer.is_empty()),
            ..Environment::default()
        }
    }

    fn status(&self) -> Result<CaptureStatus> {
        match self.manifest.status.as_str() {
            "complete" => Ok(CaptureStatus::Complete),
            "blocked" => Ok(CaptureStatus::Blocked),
            other => bail!("unknown capture status `{other}`"),
        }
    }

    /// Uploads every file and builds the capture description.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be read or uploaded.
    pub async fn describe(&self, remote: &Remote) -> Result<NewCapture> {
        let manifest = &self.manifest;
        let mut checkpoints = Vec::with_capacity(manifest.checkpoints.len());
        for checkpoint in &manifest.checkpoints {
            checkpoints.push(NewCheckpoint {
                id: checkpoint.id.clone(),
                file: self.upload(remote, &checkpoint.path).await?,
            });
        }
        let mut clips = Vec::with_capacity(manifest.clips.len());
        for clip in &manifest.clips {
            let mut frames = Vec::with_capacity(clip.frames.len());
            for frame in &clip.frames {
                frames.push(NewFrame {
                    file: self.upload(remote, &frame.path).await?,
                    target_ms: frame.target_ms,
                    video_ms: frame.video_ms,
                });
            }
            clips.push(NewClip {
                id: clip.id.clone(),
                video: self
                    .upload_optional(remote, clip.video_path.as_ref())
                    .await?,
                sheet: self
                    .upload_optional(remote, clip.sheet_path.as_ref())
                    .await?,
                analysis: self
                    .upload_optional(remote, clip.analysis_path.as_ref())
                    .await?,
                frames,
            });
        }
        Ok(NewCapture {
            scenario: manifest.scenario.clone(),
            revision: manifest.revision.clone(),
            status: self.status()?,
            blocker: manifest.blocker.clone(),
            environment: Self::environment(&manifest.environment),
            checkpoints,
            clips,
        })
    }
}
