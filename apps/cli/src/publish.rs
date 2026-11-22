//! Publishing a capture directory (a `manifest.json` and its media files) to the backend.

use crate::{
    mcp_bridge::{Remote, SERVER_URL_ENV},
    service,
};
use anyhow::{Context, Result, bail};
use rupicola_api::{
    BlobRef, CaptureStatus, Environment, NewCapture, NewCheckpoint, NewClip, NewFrame,
};
use rupicola_client::Api;
use rupicola_core::capture_manifest::{self, Manifest};
use std::path::{Component, Path, PathBuf};
use tempfile::TempDir;

/// Largest manifest read, in bytes.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Manifest format this command understands.
const MANIFEST_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";
/// Prefix of the scratch directory a capture uses when it is published.
const TEMP_PREFIX: &str = "rupicola-capture-";
/// Run directory inside the scratch directory; capture commands need it not to exist.
const RUN_DIR: &str = "run";

/// Where a finished capture goes: the backend named by `--server-url`, or only the disk.
#[derive(clap::Args)]
pub struct Destination {
    /// Base URL of the Rupicola backend; when set the capture is published there.
    #[arg(long, env = SERVER_URL_ENV)]
    server_url: Option<String>,
}

/// The directory a capture is written to, removed with the value when it was scratch.
pub struct Run {
    dir: PathBuf,
    _scratch: Option<TempDir>,
}

impl Run {
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Destination {
    /// The directory to capture into: `explicit` when given; otherwise a scratch
    /// directory when the capture will be published, or the local data directory.
    ///
    /// # Errors
    ///
    /// Fails when the directory cannot be created.
    pub fn run_dir(&self, explicit: Option<PathBuf>) -> Result<Run> {
        match (explicit, &self.server_url) {
            (Some(dir), _) => Ok(Run {
                dir,
                _scratch: None,
            }),
            (None, Some(_)) => {
                let scratch = tempfile::Builder::new().prefix(TEMP_PREFIX).tempdir()?;
                Ok(Run {
                    dir: scratch.path().join(RUN_DIR),
                    _scratch: Some(scratch),
                })
            }
            (None, None) => Ok(Run {
                dir: service::output()?,
                _scratch: None,
            }),
        }
    }

    /// Publishes the capture in `dir` when a backend is configured; does nothing otherwise.
    /// The confirmation goes to stderr when `quiet`, so JSON output stays parseable.
    ///
    /// # Errors
    ///
    /// Fails when the capture cannot be read, uploaded or accepted.
    pub async fn publish(&self, dir: &Path, quiet: bool) -> Result<()> {
        let Some(server_url) = &self.server_url else {
            return Ok(());
        };
        let bundle = Bundle::open(dir).await?;
        let project = bundle.project();
        if project.is_empty() {
            bail!("the capture names no project");
        }
        let remote = Remote::connect(server_url)?;
        let capture = remote
            .api
            .publish_capture(&remote.token, project, &bundle.describe(&remote).await?)
            .await?;
        let message = format!("Published capture {} to project {project}", capture.id);
        if quiet {
            eprintln!("{message}");
        } else {
            println!("{message}");
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: &str = "http://127.0.0.1:1";

    fn remote() -> Destination {
        Destination {
            server_url: Some(SERVER.to_owned()),
        }
    }

    #[test]
    fn an_explicit_directory_is_used_as_given() {
        let explicit = PathBuf::from("captures/run-1");
        let run = remote().run_dir(Some(explicit.clone())).unwrap();
        assert_eq!(run.dir(), explicit);
    }

    #[test]
    fn a_published_capture_without_a_directory_uses_scratch_that_goes_away() {
        let run = remote().run_dir(None).unwrap();
        let scratch = run.dir().parent().unwrap().to_path_buf();
        assert!(scratch.is_dir());
        assert!(
            !run.dir().exists(),
            "capture commands create the run directory themselves"
        );
        drop(run);
        assert!(!scratch.exists());
    }

    #[tokio::test]
    async fn publishing_without_a_backend_does_nothing() {
        let local = Destination { server_url: None };
        local
            .publish(Path::new("does-not-exist"), false)
            .await
            .unwrap();
    }
}
