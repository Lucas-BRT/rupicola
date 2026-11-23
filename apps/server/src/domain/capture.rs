//! What a published capture must look like: its scenario, the files it names, and the
//! notes reviewers leave on it.

use super::{ApiError, blob::valid_digest};
use rupicola_api::{BlobRef, CaptureStatus, NewCapture};
use std::collections::HashSet;

/// Longest file path inside a capture, and deepest nesting (slashes).
const MAX_PATH_LEN: usize = 180;
const MAX_PATH_DEPTH: usize = 4;
/// Longest checkpoint or clip id.
const MAX_NAME_LEN: usize = 128;
/// Longest scenario name.
const MAX_SCENARIO_CHARS: usize = 512;
/// Longest review note.
const MAX_NOTE_CHARS: usize = 8000;

/// Paths inside a capture: short, relative, plain names with a media extension.
fn valid_path(path: &str) -> bool {
    path.len() <= MAX_PATH_LEN
        && path.matches('/').count() <= MAX_PATH_DEPTH
        && matches!(
            std::path::Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str()),
            Some("png" | "gif" | "mp4" | "webm" | "json")
        )
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('.')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
}

fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&name.len())
}

/// One file of a capture about to be kept, with the part of the capture it belongs to.
pub struct Planned<'a> {
    pub role: &'static str,
    pub owner: &'a str,
    pub file: &'a BlobRef,
    pub target_ms: Option<f64>,
    pub video_ms: Option<f64>,
}

/// The files of `form`, in order, once each is checked: at most `max_artifacts`, each
/// with a plain relative path used once and a SHA-256 digest.
pub fn plan(form: &NewCapture, max_artifacts: usize) -> Result<Vec<Planned<'_>>, ApiError> {
    let mut planned = Vec::new();
    for checkpoint in &form.checkpoints {
        planned.push(Planned {
            role: "checkpoint",
            owner: &checkpoint.id,
            file: &checkpoint.file,
            target_ms: None,
            video_ms: None,
        });
    }
    for clip in &form.clips {
        for (role, file) in [
            ("clip-video", &clip.video),
            ("clip-sheet", &clip.sheet),
            ("clip-analysis", &clip.analysis),
        ] {
            if let Some(file) = file {
                planned.push(Planned {
                    role,
                    owner: &clip.id,
                    file,
                    target_ms: None,
                    video_ms: None,
                });
            }
        }
        for frame in &clip.frames {
            planned.push(Planned {
                role: "clip-frame",
                owner: &clip.id,
                file: &frame.file,
                target_ms: Some(frame.target_ms),
                video_ms: frame.video_ms,
            });
        }
    }
    let mut paths = HashSet::new();
    if planned.len() > max_artifacts
        || planned.iter().any(|item| {
            !valid_name(item.owner)
                || !valid_path(&item.file.path)
                || !valid_digest(&item.file.sha256)
                || !paths.insert(item.file.path.as_str())
        })
    {
        return Err(ApiError::invalid(format!(
            "a capture needs at most {max_artifacts} files with unique, plain relative paths \
                 and valid SHA-256 digests"
        )));
    }
    Ok(planned)
}

pub fn check_scenario(scenario: &str) -> Result<(), ApiError> {
    if (1..=MAX_SCENARIO_CHARS).contains(&scenario.chars().count()) {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "the scenario must have 1–{MAX_SCENARIO_CHARS} characters"
        )))
    }
}

/// A review note, trimmed, once it has 1 to [`MAX_NOTE_CHARS`] characters.
pub fn clean_note(note: &str) -> Result<&str, ApiError> {
    let note = note.trim();
    if (1..=MAX_NOTE_CHARS).contains(&note.chars().count()) {
        Ok(note)
    } else {
        Err(ApiError::invalid(format!(
            "the note must have 1–{MAX_NOTE_CHARS} characters"
        )))
    }
}

pub const fn status_text(status: CaptureStatus) -> &'static str {
    match status {
        CaptureStatus::Complete => "complete",
        CaptureStatus::Blocked => "blocked",
    }
}
