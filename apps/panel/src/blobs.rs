//! Content-addressed media storage: every file lives under its SHA-256.
//!
//! Identical files are stored once. Imports hard-link when source and store share a
//! filesystem, so moving existing captures in costs no extra disk writes.

use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub struct Blobs {
    root: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Stored {
    pub sha256: String,
    pub bytes: u64,
}

impl Blobs {
    /// Opens (and creates) the store at `root`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn open(root: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Stores a copy of `source`, hard-linking it when possible.
    ///
    /// # Errors
    ///
    /// Returns an error if `source` cannot be read or the blob cannot be written.
    pub fn put_file(&self, source: &Path) -> io::Result<Stored> {
        let mut hasher = Sha256::new();
        let bytes = io::copy(&mut fs::File::open(source)?, &mut hasher)?;
        let sha256 = format!("{:x}", hasher.finalize());
        let target = self.target(&sha256)?;
        if !target.is_file() && fs::hard_link(source, &target).is_err() {
            let staged = tempfile::NamedTempFile::new_in(target.parent().unwrap_or(&self.root))?;
            fs::copy(source, staged.path())?;
            persist(staged, &target)?;
        }
        Ok(Stored { sha256, bytes })
    }

    /// Returns the path of a stored blob, or `None` for an invalid or missing digest.
    #[must_use]
    pub fn path(&self, sha256: &str) -> Option<PathBuf> {
        valid(sha256)
            .then(|| self.root.join(&sha256[..2]).join(sha256))
            .filter(|path| path.is_file())
    }

    fn target(&self, sha256: &str) -> io::Result<PathBuf> {
        let dir = self.root.join(&sha256[..2]);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(sha256))
    }
}

fn persist(staged: tempfile::NamedTempFile, target: &Path) -> io::Result<()> {
    match staged.persist_noclobber(target) {
        Ok(_) => Ok(()),
        // Another writer stored the same content first.
        Err(_) if target.is_file() => Ok(()),
        Err(error) => Err(error.error),
    }
}

fn valid(sha256: &str) -> bool {
    sha256.len() == 64
        && sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Media type served for an artifact path.
#[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_is_stored_once_and_found_by_digest() {
        let dir = tempfile::tempdir().unwrap();
        let blobs = Blobs::open(dir.path().join("blobs")).unwrap();
        let source = dir.path().join("shot.png");
        let copy = dir.path().join("copy.png");
        fs::write(&source, b"pixels").unwrap();
        fs::write(&copy, b"pixels").unwrap();

        let from_file = blobs.put_file(&source).unwrap();
        let from_copy = blobs.put_file(&copy).unwrap();

        assert_eq!(from_file, from_copy);
        assert_eq!(
            fs::read_dir(blobs.root.join(&from_file.sha256[..2]))
                .unwrap()
                .count(),
            1
        );
        assert_eq!(from_file.bytes, 6);
        let path = blobs.path(&from_file.sha256).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"pixels");
    }

    #[test]
    fn rejects_invalid_or_unknown_digests() {
        let dir = tempfile::tempdir().unwrap();
        let blobs = Blobs::open(dir.path().to_path_buf()).unwrap();
        assert!(blobs.path("../etc/passwd").is_none());
        assert!(blobs.path(&"A".repeat(64)).is_none());
        assert!(blobs.path(&"a".repeat(64)).is_none());
    }

    #[test]
    fn maps_media_types_by_extension() {
        assert_eq!(mime("checkpoints/home.png"), "image/png");
        assert_eq!(mime("clips/open.webm"), "video/webm");
        assert_eq!(mime("notes.txt"), "application/octet-stream");
    }
}
