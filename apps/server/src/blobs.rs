//! Content-addressed media store: every file is kept once, under its SHA-256.
//! Disk in production; memory in tests.

use crate::domain::ApiError;
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
    sha256.len() == crate::domain::secret::HEX_SECRET_LEN
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

/// Bytes read from the start of an upload to tell what kind of file it is.
pub const MAGIC_LEN: usize = 12;
const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G'];
const GIF_MAGIC: &[u8] = b"GIF8";
const WEBM_MAGIC: &[u8] = &[0x1a, 0x45, 0xdf, 0xa3];
/// An MP4 names its type in bytes 4 to 8.
const MP4_BRAND_AT: std::ops::Range<usize> = 4..8;
const MP4_BRAND: &[u8] = b"ftyp";

/// What an upload is, by its first bytes, never by what the client says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobKind {
    Image,
    Video,
    Other,
}

impl BlobKind {
    /// The kind of a file that starts with `head`.
    pub fn of(head: &[u8]) -> Self {
        if head.starts_with(PNG_MAGIC) || head.starts_with(GIF_MAGIC) {
            Self::Image
        } else if head.starts_with(WEBM_MAGIC) || head.get(MP4_BRAND_AT) == Some(MP4_BRAND) {
            Self::Video
        } else {
            Self::Other
        }
    }

    /// The largest file of this kind `limits` allow.
    pub fn limit(self, limits: &rupicola_api::Limits) -> u64 {
        match self {
            Self::Image => limits.max_image_bytes,
            Self::Video => limits.max_video_bytes,
            Self::Other => limits.max_blob_bytes,
        }
        .min(limits.max_blob_bytes)
    }
}

/// A file being received: written to a temporary file next to where it will live (never in
/// `/tmp`, which may be small or another filesystem), hashed as it arrives, and stored only
/// when its digest is the one it was sent under.
pub struct Upload {
    sha256: String,
    hasher: Sha256,
    bytes: u64,
    staging: Staging,
}

enum Staging {
    Disk {
        file: tokio::fs::File,
        temp: tempfile::TempPath,
        target: PathBuf,
    },
    #[cfg(test)]
    Memory(Vec<u8>),
}

impl Blobs {
    /// Starts receiving the file whose digest is `sha256`.
    ///
    /// # Errors
    ///
    /// [`ApiError::Invalid`] when `sha256` is not a digest; an internal error when the
    /// temporary file cannot be made.
    pub fn begin(&self, sha256: &str) -> Result<Upload, ApiError> {
        if !valid_digest(sha256) {
            return Err(ApiError::invalid("the upload is not named by a SHA-256"));
        }
        let staging = match self {
            Self::Disk(root) => {
                let target = disk_path(root, sha256);
                let dir = target.parent().unwrap_or(root);
                fs::create_dir_all(dir).map_err(anyhow::Error::from)?;
                let (file, temp) = tempfile::NamedTempFile::new_in(dir)
                    .map_err(anyhow::Error::from)?
                    .into_parts();
                Staging::Disk {
                    file: tokio::fs::File::from_std(file),
                    temp,
                    target,
                }
            }
            #[cfg(test)]
            Self::Memory(_) => Staging::Memory(Vec::new()),
        };
        Ok(Upload {
            sha256: sha256.to_owned(),
            hasher: Sha256::new(),
            bytes: 0,
            staging,
        })
    }
}

impl Upload {
    /// Bytes received so far.
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Takes the next piece of the file.
    ///
    /// # Errors
    ///
    /// When the temporary file cannot be written.
    pub async fn write(&mut self, chunk: &[u8]) -> Result<(), ApiError> {
        self.hasher.update(chunk);
        self.bytes += u64::try_from(chunk.len()).map_err(anyhow::Error::from)?;
        match &mut self.staging {
            Staging::Disk { file, .. } => {
                tokio::io::AsyncWriteExt::write_all(file, chunk)
                    .await
                    .map_err(anyhow::Error::from)?;
            }
            #[cfg(test)]
            Staging::Memory(data) => data.extend_from_slice(chunk),
        }
        Ok(())
    }

    /// Stores the file if its digest is the one it was sent under. Returns whether it was
    /// new; a file already stored is kept as it was.
    ///
    /// # Errors
    ///
    /// [`ApiError::Invalid`] when the content does not match the digest; an internal error
    /// when it cannot be stored.
    pub async fn finish(
        self,
        #[cfg_attr(
            not(test),
            expect(unused_variables, reason = "only the in-memory test store needs it")
        )]
        store: &Blobs,
    ) -> Result<bool, ApiError> {
        if format!("{:x}", self.hasher.finalize()) != self.sha256 {
            return Err(ApiError::invalid(
                "the uploaded content does not match its SHA-256",
            ));
        }
        match self.staging {
            Staging::Disk {
                file, temp, target, ..
            } => {
                file.sync_all().await.map_err(anyhow::Error::from)?;
                drop(file);
                if target.is_file() {
                    return Ok(false);
                }
                match temp.persist_noclobber(&target) {
                    Ok(()) => Ok(true),
                    // Another upload stored the same content first.
                    Err(_) if target.is_file() => Ok(false),
                    Err(error) => Err(anyhow::Error::from(error.error).into()),
                }
            }
            #[cfg(test)]
            Staging::Memory(data) => match store {
                Blobs::Memory(map) => Ok(map
                    .lock()
                    .map_err(|_| anyhow::anyhow!("the blob store lock is poisoned"))?
                    .insert(self.sha256, data)
                    .is_none()),
                Blobs::Disk(_) => Err(anyhow::anyhow!("a memory upload into a disk store").into()),
            },
        }
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
        let now = crate::domain::secret::now_ms();
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

#[cfg(test)]
mod upload_tests {
    use super::*;

    fn digest(data: &[u8]) -> String {
        format!("{:x}", Sha256::digest(data))
    }

    #[tokio::test]
    async fn a_streamed_file_lands_on_disk_once_and_a_wrong_one_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = Blobs::disk(dir.path().join("blobs")).unwrap();
        let data = b"pixels arriving in pieces";
        let sha = digest(data);
        let mut upload = store.begin(&sha).unwrap();
        for piece in data.chunks(7) {
            upload.write(piece).await.unwrap();
        }
        assert_eq!(upload.bytes(), 25);
        assert!(
            upload.finish(&store).await.unwrap(),
            "the first copy is new"
        );
        assert_eq!(store.size(&sha).unwrap(), Some(25));
        let mut again = store.begin(&sha).unwrap();
        again.write(data).await.unwrap();
        assert!(
            !again.finish(&store).await.unwrap(),
            "the second copy is not"
        );
        let wrong = digest(b"promised");
        let mut lie = store.begin(&wrong).unwrap();
        lie.write(b"delivered").await.unwrap();
        assert!(lie.finish(&store).await.is_err());
        assert_eq!(store.size(&wrong).unwrap(), None);
        let leftovers: Vec<_> = walk(dir.path())
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".tmp"))
            })
            .collect();
        assert_eq!(
            leftovers,
            Vec::<PathBuf>::new(),
            "no temporary file is left behind"
        );
    }

    #[test]
    fn the_kind_of_a_file_comes_from_its_first_bytes() {
        assert_eq!(
            BlobKind::of(&[0x89, b'P', b'N', b'G', 0, 0]),
            BlobKind::Image
        );
        assert_eq!(BlobKind::of(b"GIF89a......"), BlobKind::Image);
        assert_eq!(
            BlobKind::of(&[0x1a, 0x45, 0xdf, 0xa3, 1, 2]),
            BlobKind::Video
        );
        assert_eq!(BlobKind::of(b"\0\0\0\x18ftypmp42"), BlobKind::Video);
        assert_eq!(BlobKind::of(b"{\"frames\":[]}"), BlobKind::Other);
        assert_eq!(BlobKind::of(b""), BlobKind::Other);
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path);
            }
        }
        found
    }
}
