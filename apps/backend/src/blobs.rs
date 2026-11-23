//! Content-addressed media store: every file is kept once, under its SHA-256.
//! Disk in production; memory in tests.

use crate::error::ApiError;
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, header},
    response::Response,
};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub enum Blobs {
    Disk(PathBuf),
    #[cfg(test)]
    Memory(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>),
}

/// Whether `sha256` is a lowercase hex SHA-256 digest, safe as a file name.
pub fn valid_digest(sha256: &str) -> bool {
    sha256.len() == crate::auth::HEX_SECRET_LEN
        && sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl Blobs {
    /// A store rooted at `root`, created if missing.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn disk(root: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self::Disk(root))
    }

    #[cfg(test)]
    pub fn memory() -> Self {
        Self::Memory(std::sync::Mutex::default())
    }

    /// Stores `data` if its digest is `sha256`. Returns whether it was new.
    ///
    /// # Errors
    ///
    /// [`ApiError::Invalid`] when the content does not match the digest.
    pub fn put(&self, sha256: &str, data: &[u8]) -> Result<bool, ApiError> {
        if !valid_digest(sha256) || format!("{:x}", Sha256::digest(data)) != sha256 {
            return Err(ApiError::invalid(
                "the uploaded content does not match its SHA-256",
            ));
        }
        match self {
            Self::Disk(root) => {
                let target = disk_path(root, sha256);
                if target.is_file() {
                    return Ok(false);
                }
                let dir = target.parent().unwrap_or(root);
                fs::create_dir_all(dir).map_err(anyhow::Error::from)?;
                let mut staged =
                    tempfile::NamedTempFile::new_in(dir).map_err(anyhow::Error::from)?;
                io::Write::write_all(&mut staged, data).map_err(anyhow::Error::from)?;
                staged.as_file().sync_all().map_err(anyhow::Error::from)?;
                match staged.persist_noclobber(&target) {
                    Ok(_) => Ok(true),
                    // Another upload stored the same content first.
                    Err(_) if target.is_file() => Ok(false),
                    Err(error) => Err(anyhow::Error::from(error.error).into()),
                }
            }
            #[cfg(test)]
            Self::Memory(map) => Ok(map
                .lock()
                .expect("blob map poisoned")
                .insert(sha256.to_owned(), data.to_vec())
                .is_none()),
        }
    }

    /// Size of a stored blob, or `None` when it is missing.
    ///
    /// # Errors
    ///
    /// Returns an error if the disk cannot be read.
    pub fn size(&self, sha256: &str) -> io::Result<Option<u64>> {
        if !valid_digest(sha256) {
            return Ok(None);
        }
        match self {
            Self::Disk(root) => match fs::metadata(disk_path(root, sha256)) {
                Ok(meta) => Ok(Some(meta.len())),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            },
            #[cfg(test)]
            Self::Memory(map) => Ok(map
                .lock()
                .expect("blob map poisoned")
                .get(sha256)
                .map(|data| data.len() as u64)),
        }
    }

    /// Up to `length` bytes of the blob starting at `offset`; shorter (or empty) at the end.
    ///
    /// # Errors
    ///
    /// [`ApiError::NotFound`] when the blob is missing.
    pub fn read_range(
        &self,
        sha256: &str,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, ApiError> {
        if !valid_digest(sha256) {
            return Err(ApiError::NotFound);
        }
        match self {
            Self::Disk(root) => {
                use std::io::{Read, Seek, SeekFrom};
                let mut file = fs::File::open(disk_path(root, sha256)).map_err(|error| {
                    if error.kind() == io::ErrorKind::NotFound {
                        ApiError::NotFound
                    } else {
                        ApiError::from(anyhow::Error::from(error))
                    }
                })?;
                file.seek(SeekFrom::Start(offset))
                    .map_err(anyhow::Error::from)?;
                let mut data = Vec::new();
                file.take(length as u64)
                    .read_to_end(&mut data)
                    .map_err(anyhow::Error::from)?;
                Ok(data)
            }
            #[cfg(test)]
            Self::Memory(map) => {
                let map = map.lock().expect("blob map poisoned");
                let data = map.get(sha256).ok_or(ApiError::NotFound)?;
                let start = usize::try_from(offset).map_or(data.len(), |o| o.min(data.len()));
                let end = start.saturating_add(length).min(data.len());
                Ok(data[start..end].to_vec())
            }
        }
    }

    /// Answers `request` with the blob's bytes as `mime`. On disk this supports
    /// range requests (video seeking) and conditional requests.
    ///
    /// # Errors
    ///
    /// [`ApiError::NotFound`] when the blob is missing.
    pub async fn respond(
        &self,
        sha256: &str,
        mime: &str,
        request: Request,
    ) -> Result<Response, ApiError> {
        let mime = HeaderValue::from_str(mime).map_err(anyhow::Error::from)?;
        let mut response = match self {
            Self::Disk(root) => {
                let path = disk_path(root, sha256);
                if !valid_digest(sha256) || !path.is_file() {
                    return Err(ApiError::NotFound);
                }
                ServeFile::new(path)
                    .oneshot(request)
                    .await
                    .map_err(anyhow::Error::from)?
                    .map(Body::new)
            }
            #[cfg(test)]
            Self::Memory(map) => {
                drop(request);
                let data = map
                    .lock()
                    .expect("blob map poisoned")
                    .get(sha256)
                    .cloned()
                    .ok_or(ApiError::NotFound)?;
                axum::response::IntoResponse::into_response(data)
            }
        };
        // Blobs carry no extension, so the recorded media type replaces any guess.
        response.headers_mut().insert(header::CONTENT_TYPE, mime);
        Ok(response)
    }
}

/// Leading hex characters naming the directory a blob lives in, so no single
/// directory grows too large.
const FANOUT_CHARS: usize = 2;

fn disk_path(root: &Path, sha256: &str) -> PathBuf {
    root.join(&sha256[..FANOUT_CHARS]).join(sha256)
}

/// Media type served for an artifact path.
pub fn mime(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

/// Which blobs exist and which projects know them. A project only learns of a blob it
/// uploaded or a capture of it cites, so asking what is missing reveals nothing about other
/// projects.
pub struct BlobIndex;

impl BlobIndex {
    /// Records that `project` holds the blob `sha256` of `bytes` bytes.
    ///
    /// # Errors
    ///
    /// When the database refuses the rows.
    pub async fn record(
        tx: &mut sqlx::SqliteConnection,
        project: &str,
        sha256: &str,
        bytes: u64,
    ) -> Result<(), ApiError> {
        let now = crate::auth::now_ms();
        sqlx::query("INSERT OR IGNORE INTO blobs (sha256, bytes, created_at) VALUES (?1, ?2, ?3)")
            .bind(sha256)
            .bind(i64::try_from(bytes).map_err(anyhow::Error::from)?)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO project_blobs (project_id, sha256, created_at)
             VALUES (?1, ?2, ?3)",
        )
        .bind(project)
        .bind(sha256)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        Ok(())
    }

    /// Of `hashes`, those `project` does not hold, in the order given and each once.
    ///
    /// # Errors
    ///
    /// When the database cannot be read.
    pub async fn missing(
        db: &sqlx::SqlitePool,
        project: &str,
        hashes: &[String],
    ) -> Result<Vec<String>, ApiError> {
        let asked = serde_json::to_string(hashes).map_err(anyhow::Error::from)?;
        let known: std::collections::HashSet<String> = sqlx::query_scalar(
            "SELECT sha256 FROM project_blobs
             WHERE project_id = ?1 AND sha256 IN (SELECT value FROM json_each(?2))",
        )
        .bind(project)
        .bind(asked)
        .fetch_all(db)
        .await?
        .into_iter()
        .collect();
        let mut seen = std::collections::HashSet::new();
        Ok(hashes
            .iter()
            .filter(|hash| !known.contains(*hash) && seen.insert(hash.as_str()))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(data: &[u8]) -> String {
        format!("{:x}", Sha256::digest(data))
    }

    #[test]
    fn stores_content_once_and_rejects_a_wrong_digest() {
        let blobs = Blobs::memory();
        let sha = digest(b"pixels");
        assert!(blobs.put(&sha, b"pixels").unwrap());
        assert!(!blobs.put(&sha, b"pixels").unwrap());
        assert_eq!(blobs.size(&sha).unwrap(), Some(b"pixels".len() as u64));
        assert!(blobs.put(&sha, b"other").is_err());
        assert!(blobs.put("../etc/passwd", b"x").is_err());
        assert_eq!(blobs.size(&digest(b"missing")).unwrap(), None);
    }

    #[test]
    fn maps_media_types_by_extension() {
        assert_eq!(mime("checkpoints/home.png"), "image/png");
        assert_eq!(mime("clips/open.webm"), "video/webm");
        assert_eq!(mime("notes.txt"), "application/octet-stream");
    }
}
