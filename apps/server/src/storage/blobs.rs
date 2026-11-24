//! Content-addressed media store: every file is kept once, under its SHA-256.
//! Disk in production; memory in tests. What a file is and which project holds it are
//! decided elsewhere: [`crate::domain::blob`] and [`crate::repositories::blobs`].

use crate::domain::{ApiError, blob::valid_digest};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub enum Blobs {
    Disk(PathBuf),
    #[cfg(test)]
    Memory(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>),
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

    /// Where the blob is kept, for answering a request with it.
    ///
    /// # Errors
    ///
    /// [`ApiError::NotFound`] when the blob is missing.
    pub fn locate(&self, sha256: &str) -> Result<Stored, ApiError> {
        if !valid_digest(sha256) {
            return Err(ApiError::NotFound);
        }
        match self {
            Self::Disk(root) => {
                let path = disk_path(root, sha256);
                if path.is_file() {
                    Ok(Stored::File(path))
                } else {
                    Err(ApiError::NotFound)
                }
            }
            #[cfg(test)]
            Self::Memory(map) => map
                .lock()
                .expect("blob map poisoned")
                .get(sha256)
                .cloned()
                .map(Stored::Bytes)
                .ok_or(ApiError::NotFound),
        }
    }
}

/// A stored blob: a file on disk, or its bytes in the test store.
pub enum Stored {
    File(PathBuf),
    #[cfg(test)]
    Bytes(Vec<u8>),
}

/// Leading hex characters naming the directory a blob lives in, so no single
/// directory grows too large.
const FANOUT_CHARS: usize = 2;

fn disk_path(root: &Path, sha256: &str) -> PathBuf {
    root.join(&sha256[..FANOUT_CHARS]).join(sha256)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::blob::mime;

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
    use crate::domain::blob::BlobKind;

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
