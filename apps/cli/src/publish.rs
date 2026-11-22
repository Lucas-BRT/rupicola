//! Publishing a capture directory (a `manifest.json` and its media files) to the backend.

use crate::api::Client;
use anyhow::{Context, Result, bail};
use rupicola_api::{
    BlobRef, CaptureStatus, Environment, NewCapture, NewCheckpoint, NewClip, NewFrame,
};
use rupicola_core::capture_manifest::{self, Manifest};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

/// Largest manifest read, in bytes.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Manifest format this command understands.
const MANIFEST_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";

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
    pub fn open(dir: &Path) -> Result<Self> {
        let root = dir
            .canonicalize()
            .with_context(|| format!("open {}", dir.display()))?;
        let path = root.join(MANIFEST_FILE);
        if fs::metadata(&path)?.len() > MAX_MANIFEST_BYTES {
            bail!("{MANIFEST_FILE} is larger than {MAX_MANIFEST_BYTES} bytes");
        }
        let manifest: Manifest = serde_json::from_slice(&fs::read(&path)?)
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
    fn upload(&self, client: &Client, relative: &str) -> Result<BlobRef> {
        let relative_path = Path::new(relative);
        if relative_path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            bail!("{relative} is not a path inside the capture");
        }
        let file = self.root.join(relative_path).canonicalize()?;
        if !file.starts_with(&self.root) || !file.is_file() {
            bail!("{relative} is not a regular file of the capture");
        }
        let sha256 = client.put_blob(&fs::read(&file)?)?;
        Ok(BlobRef {
            path: relative.to_owned(),
            sha256,
        })
    }

    fn upload_optional(
        &self,
        client: &Client,
        relative: Option<&String>,
    ) -> Result<Option<BlobRef>> {
        relative.map(|path| self.upload(client, path)).transpose()
    }

    fn environment(environment: &capture_manifest::Environment) -> Environment {
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
    pub fn describe(&self, client: &Client) -> Result<NewCapture> {
        let manifest = &self.manifest;
        let checkpoints = manifest
            .checkpoints
            .iter()
            .map(|checkpoint| {
                Ok(NewCheckpoint {
                    id: checkpoint.id.clone(),
                    file: self.upload(client, &checkpoint.path)?,
                })
            })
            .collect::<Result<_>>()?;
        let clips = manifest
            .clips
            .iter()
            .map(|clip| {
                Ok(NewClip {
                    id: clip.id.clone(),
                    video: self.upload_optional(client, clip.video_path.as_ref())?,
                    sheet: self.upload_optional(client, clip.sheet_path.as_ref())?,
                    analysis: self.upload_optional(client, clip.analysis_path.as_ref())?,
                    frames: clip
                        .frames
                        .iter()
                        .map(|frame| {
                            Ok(NewFrame {
                                file: self.upload(client, &frame.path)?,
                                target_ms: frame.target_ms,
                                video_ms: frame.video_ms,
                            })
                        })
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<_>>()?;
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
