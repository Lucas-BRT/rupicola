//! Publishing a canonical capture record directory (`record.json` and its member blobs) to the backend.

use crate::session::Remote;
use anyhow::{Context, Result, bail};
use rupicola_api::{MissingQuery, record::CaptureRecord};
use rupicola_client::Api;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

/// Largest `record.json` read, in bytes.
const MAX_RECORD_BYTES: u64 = 1024 * 1024;
const RECORD_FILE: &str = "record.json";

/// A capture directory opened for publication: its canonical [`CaptureRecord`] and member files.
pub struct Bundle {
    dir: PathBuf,
    record: CaptureRecord,
}

impl Bundle {
    /// Reads and validates `dir/record.json`.
    pub async fn open(dir: &Path) -> Result<Self> {
        let path = dir.join(RECORD_FILE);
        let meta = tokio::fs::metadata(&path)
            .await
            .with_context(|| format!("stat {}", path.display()))?;
        if meta.len() > MAX_RECORD_BYTES {
            bail!("{} exceeds {MAX_RECORD_BYTES} bytes", path.display());
        }
        let text = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("read {}", path.display()))?;
        let record: CaptureRecord =
            serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        record
            .validate()
            .with_context(|| format!("validate {}", path.display()))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            record,
        })
    }

    /// The project named inside the record.
    #[must_use]
    pub fn project(&self) -> &str {
        &self.record.project
    }

    /// Uploads every missing member blob to `project` and returns the canonical [`CaptureRecord`].
    pub async fn upload_and_record(&self, remote: &Remote, project: &str) -> Result<CaptureRecord> {
        let mut by_sha: BTreeMap<String, PathBuf> = BTreeMap::new();
        for member in &self.record.members {
            let rel = safe_relative(&member.path)?;
            by_sha
                .entry(member.sha256.clone())
                .or_insert_with(|| self.dir.join(rel));
        }

        if !by_sha.is_empty() {
            let hashes: Vec<String> = by_sha.keys().cloned().collect();
            let missing = remote
                .api
                .missing_blobs(&remote.token, project, &MissingQuery { sha256: hashes })
                .await?;
            let needed: BTreeSet<String> = missing.missing.into_iter().collect();
            for (sha256, path) in &by_sha {
                if !needed.contains(sha256) {
                    continue;
                }
                let data = tokio::fs::read(path)
                    .await
                    .with_context(|| format!("read member {}", path.display()))?;
                remote
                    .api
                    .upload_project_blob(&remote.token, project, sha256, data)
                    .await
                    .with_context(|| format!("upload blob {sha256}"))?;
            }
        }

        let mut record = self.record.clone();
        project.clone_into(&mut record.project);
        Ok(record)
    }
}

fn safe_relative(raw: &str) -> Result<PathBuf> {
    let path = Path::new(raw);
    if path.is_absolute() {
        bail!("member path `{raw}` must be relative");
    }
    let mut clean = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => bail!("member path `{raw}` escapes the capture directory"),
        }
    }
    if clean.as_os_str().is_empty() {
        bail!("member path `{raw}` is empty");
    }
    Ok(clean)
}
