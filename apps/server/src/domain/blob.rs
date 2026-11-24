//! What an uploaded file is: its digest, its kind by its first bytes, and the media type
//! it is served as.

use std::path::Path;

/// Whether `sha256` is a lowercase hex SHA-256 digest, safe as a file name.
pub fn valid_digest(sha256: &str) -> bool {
    sha256.len() == super::secret::HEX_SECRET_LEN
        && sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
