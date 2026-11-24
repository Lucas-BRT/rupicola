//! Publishing a capture directory (a `manifest.json` and its media files) to the backend.

use crate::session::Remote;
use anyhow::{Context, Result, bail};
use rupicola_api::{
    BlobRef, CaptureStatus, Environment, MissingQuery, NewCapture, NewCheckpoint, NewClip, NewFrame,
};
use rupicola_client::Api;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};
use tokio::io::AsyncReadExt as _;

/// Largest manifest read, in bytes.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Buffer size used when hashing capture media files.
const HASH_CHUNK_BYTES: usize = 64 * 1024;
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

    /// Validates that `relative` points to a regular file inside the capture root.
    async fn resolve_file(&self, relative: &str) -> Result<PathBuf> {
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
        Ok(file)
    }

    async fn hash_file(path: &Path) -> Result<String> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0_u8; HASH_CHUNK_BYTES];
        loop {
            let read = file.read(&mut buf).await?;
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn collect_paths(&self) -> Vec<&str> {
        let mut paths = Vec::new();
        for checkpoint in &self.manifest.checkpoints {
            paths.push(checkpoint.path.as_str());
        }
        for clip in &self.manifest.clips {
            if let Some(path) = clip.video_path.as_deref() {
                paths.push(path);
            }
            if let Some(path) = clip.sheet_path.as_deref() {
                paths.push(path);
            }
            if let Some(path) = clip.analysis_path.as_deref() {
                paths.push(path);
            }
            for frame in &clip.frames {
                paths.push(frame.path.as_str());
            }
        }
        paths
    }

    fn blob_ref(blobs: &BTreeMap<String, (PathBuf, String)>, relative: &str) -> Result<BlobRef> {
        let (_, sha256) = blobs
            .get(relative)
            .with_context(|| format!("missing resolved blob for {relative}"))?;
        Ok(BlobRef {
            path: relative.to_owned(),
            sha256: sha256.clone(),
        })
    }

    fn optional_blob_ref(
        blobs: &BTreeMap<String, (PathBuf, String)>,
        relative: Option<&String>,
    ) -> Result<Option<BlobRef>> {
        relative.map(|path| Self::blob_ref(blobs, path)).transpose()
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

    /// Uploads missing files in a single batch and builds the capture description.
    ///
    /// # Errors
    ///
    /// Fails when a file cannot be read or uploaded.
    pub async fn describe(&self, remote: &Remote) -> Result<NewCapture> {
        let mut blobs = BTreeMap::new();
        for relative in self.collect_paths() {
            if !blobs.contains_key(relative) {
                let file = self.resolve_file(relative).await?;
                let sha256 = Self::hash_file(&file).await?;
                blobs.insert(relative.to_owned(), (file, sha256));
            }
        }
        let unique_hashes: Vec<String> = blobs
            .values()
            .map(|(_, sha256)| sha256.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !unique_hashes.is_empty() {
            let missing = remote
                .api
                .missing_blobs(
                    &remote.token,
                    self.project(),
                    &MissingQuery {
                        sha256: unique_hashes,
                    },
                )
                .await?;
            let missing_set: BTreeSet<String> = missing.missing.into_iter().collect();
            let mut uploaded = BTreeSet::new();
            for (file, sha256) in blobs.values() {
                if missing_set.contains(sha256) && uploaded.insert(sha256.clone()) {
                    let data = tokio::fs::read(file).await?;
                    remote
                        .api
                        .upload_project_blob(&remote.token, self.project(), sha256, data)
                        .await?;
                }
            }
        }
        let manifest = &self.manifest;
        let mut checkpoints = Vec::with_capacity(manifest.checkpoints.len());
        for checkpoint in &manifest.checkpoints {
            checkpoints.push(NewCheckpoint {
                id: checkpoint.id.clone(),
                file: Self::blob_ref(&blobs, &checkpoint.path)?,
            });
        }
        let mut clips = Vec::with_capacity(manifest.clips.len());
        for clip in &manifest.clips {
            let mut frames = Vec::with_capacity(clip.frames.len());
            for frame in &clip.frames {
                frames.push(NewFrame {
                    file: Self::blob_ref(&blobs, &frame.path)?,
                    target_ms: frame.target_ms,
                    video_ms: frame.video_ms,
                });
            }
            clips.push(NewClip {
                id: clip.id.clone(),
                video: Self::optional_blob_ref(&blobs, clip.video_path.as_ref())?,
                sheet: Self::optional_blob_ref(&blobs, clip.sheet_path.as_ref())?,
                analysis: Self::optional_blob_ref(&blobs, clip.analysis_path.as_ref())?,
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
