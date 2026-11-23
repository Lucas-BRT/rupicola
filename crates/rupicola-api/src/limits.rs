//! What a backend accepts, so a client can check its files before it sends any byte.

use crate::record::Member;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Most hashes one `blobs/missing` call may carry.
pub const MAX_MISSING_BATCH: usize = 1000;
/// Length of a SHA-256 written as hex.
const SHA256_HEX_LEN: usize = 64;
/// Extensions that name an image, and a video.
const IMAGE_EXTENSIONS: [&str; 2] = ["png", "gif"];
const VIDEO_EXTENSIONS: [&str; 2] = ["mp4", "webm"];

/// `GET /v1/projects/{id}/limits`: what this project may upload.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Limits {
    /// The largest file of any kind; the other file limits never exceed it.
    pub max_blob_bytes: u64,
    /// The largest image (`png`, `gif`).
    pub max_image_bytes: u64,
    /// The largest video (`mp4`, `webm`).
    pub max_video_bytes: u64,
    /// Files in one capture.
    pub max_artifacts_per_capture: u32,
    /// The largest JSON request body.
    pub max_body_bytes: u64,
    /// Hashes in one `blobs/missing` call.
    pub max_missing_batch: u32,
    /// Seconds one file upload may take in all.
    pub upload_timeout_secs: u64,
    /// Seconds an upload may go without a byte before it is cut.
    pub upload_idle_secs: u64,
}

/// A file or a capture that does not fit in the limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// A file is larger than its limit.
    TooLarge {
        path: String,
        bytes: u64,
        limit: u64,
    },
    /// The capture has more files than allowed.
    TooManyFiles { files: usize, limit: u32 },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { path, bytes, limit } => {
                write!(f, "{path} is {bytes} bytes; the limit is {limit}")
            }
            Self::TooManyFiles { files, limit } => {
                write!(f, "{files} files in one capture; the limit is {limit}")
            }
        }
    }
}

impl Limits {
    /// The largest size of the file at `path`, by its extension.
    #[must_use]
    pub fn max_bytes_for(&self, path: &str) -> u64 {
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        let kind = if IMAGE_EXTENSIONS.contains(&extension) {
            self.max_image_bytes
        } else if VIDEO_EXTENSIONS.contains(&extension) {
            self.max_video_bytes
        } else {
            self.max_blob_bytes
        };
        kind.min(self.max_blob_bytes)
    }

    /// Everything about `members` that the limits refuse, so a client can stop before it
    /// uploads anything.
    #[must_use]
    pub fn violations(&self, members: &[Member]) -> Vec<Violation> {
        let mut found = Vec::new();
        let files = members.len();
        if u32::try_from(files).map_or(true, |files| files > self.max_artifacts_per_capture) {
            found.push(Violation::TooManyFiles {
                files,
                limit: self.max_artifacts_per_capture,
            });
        }
        for member in members {
            let limit = self.max_bytes_for(&member.path);
            if member.bytes > limit {
                found.push(Violation::TooLarge {
                    path: member.path.clone(),
                    bytes: member.bytes,
                    limit,
                });
            }
        }
        found
    }
}

/// `POST /v1/projects/{id}/blobs/missing`: which of these files does the backend not have?
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct MissingQuery {
    pub sha256: Vec<String>,
}

impl MissingQuery {
    /// Checks the hashes are SHA-256 digests and there are no more than a call may carry.
    ///
    /// # Errors
    ///
    /// A sentence saying what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        if self.sha256.len() > MAX_MISSING_BATCH {
            return Err(format!("at most {MAX_MISSING_BATCH} hashes per call"));
        }
        let plain = |text: &String| {
            text.len() == SHA256_HEX_LEN
                && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        };
        match self.sha256.iter().find(|text| !plain(text)) {
            Some(bad) => Err(format!("`{bad}` is not a lowercase SHA-256")),
            None => Ok(()),
        }
    }
}

/// The answer to [`MissingQuery`]: the hashes to upload, and the limits to upload them under.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Missing {
    pub missing: Vec<String>,
    pub limits: Limits,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Member, MemberRole};
    use std::collections::BTreeMap;

    const MIB: u64 = 1024 * 1024;
    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn limits() -> Limits {
        Limits {
            max_blob_bytes: 512 * MIB,
            max_image_bytes: 32 * MIB,
            max_video_bytes: 512 * MIB,
            max_artifacts_per_capture: 3,
            max_body_bytes: MIB,
            max_missing_batch: 1000,
            upload_timeout_secs: 600,
            upload_idle_secs: 30,
        }
    }

    fn member(path: &str, bytes: u64) -> Member {
        Member {
            path: path.to_owned(),
            sha256: SHA.to_owned(),
            bytes,
            mime: "application/octet-stream".to_owned(),
            role: MemberRole::Checkpoint,
            owner: "home".to_owned(),
            surface: None,
            locator: BTreeMap::new(),
            target_ms: None,
            video_ms: None,
        }
    }

    #[test]
    fn the_limit_depends_on_what_the_file_is() {
        let limits = limits();
        assert_eq!(limits.max_bytes_for("a/home.png"), 32 * MIB);
        assert_eq!(limits.max_bytes_for("a/open.webm"), 512 * MIB);
        assert_eq!(limits.max_bytes_for("a/analysis.json"), 512 * MIB);
        assert_eq!(limits.max_bytes_for("noextension"), 512 * MIB);
    }

    #[test]
    fn no_kind_may_exceed_the_ceiling() {
        let mut limits = limits();
        limits.max_blob_bytes = 8 * MIB;
        assert_eq!(limits.max_bytes_for("a.png"), 8 * MIB);
        assert_eq!(limits.max_bytes_for("a.mp4"), 8 * MIB);
    }

    #[test]
    fn every_file_over_its_limit_is_listed_with_its_size() {
        let members = [
            member("a/ok.png", 32 * MIB),
            member("a/big.png", 32 * MIB + 1),
            member("a/clip.mp4", 100 * MIB),
        ];
        assert_eq!(
            limits().violations(&members),
            vec![Violation::TooLarge {
                path: "a/big.png".to_owned(),
                bytes: 32 * MIB + 1,
                limit: 32 * MIB
            }]
        );
    }

    #[test]
    fn too_many_files_is_reported_once() {
        let members: Vec<Member> = (0..4).map(|i| member(&format!("a/{i}.png"), 1)).collect();
        let found = limits().violations(&members);
        assert_eq!(found, vec![Violation::TooManyFiles { files: 4, limit: 3 }]);
        assert_eq!(
            found[0].to_string(),
            "4 files in one capture; the limit is 3"
        );
    }

    #[test]
    fn files_that_fit_raise_nothing() {
        assert_eq!(limits().violations(&[member("a/x.png", 1)]), vec![]);
        assert_eq!(limits().violations(&[]), vec![]);
    }

    #[test]
    fn a_missing_query_takes_only_sha256_hashes_in_a_bounded_number() {
        let query = |hashes: Vec<String>| MissingQuery { sha256: hashes };
        assert!(query(vec![]).validate().is_ok());
        assert!(query(vec![SHA.to_owned()]).validate().is_ok());
        assert!(query(vec!["ABC".to_owned()]).validate().is_err());
        assert!(query(vec![SHA.to_uppercase()]).validate().is_err());
        assert!(
            query(vec![SHA.to_owned(); MAX_MISSING_BATCH + 1])
                .validate()
                .unwrap_err()
                .contains("1000")
        );
    }

    #[test]
    fn the_answer_carries_the_limits_and_survives_a_round_trip() {
        let answer = Missing {
            missing: vec![SHA.to_owned()],
            limits: limits(),
        };
        let text = serde_json::to_string(&answer).unwrap();
        assert_eq!(serde_json::from_str::<Missing>(&text).unwrap(), answer);
    }
}
