//! Animation frame sequences: stalls, jumps and settling.

use super::{Image, diff_images, load};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// Frame analysis: fewest frames that can show motion, the per-frame change
/// below which a frame counts as still, and the jump rule (big in absolute terms
/// and several times the median step).
const MIN_FRAMES: usize = 3;
const STILL_FRAME_RATIO: f64 = 0.0005;
const JUMP_MIN_RATIO: f64 = 0.05;
const JUMP_MEDIAN_FACTOR: f64 = 4.0;

#[derive(Debug, Serialize)]
pub struct Frames {
    pub frame_count: usize,
    pub step_changed_ratio: Vec<f64>,
    /// First and last frame compared: how much the whole animation changed.
    pub total_changed_ratio: f64,
    pub first_change: Option<usize>,
    pub settled_at: Option<usize>,
    /// Frames inside the active range that repeat the previous frame (dropped or late frames).
    pub stalls: Vec<usize>,
    /// Steps that change far more than the typical step (a jump instead of a tween).
    pub jumps: Vec<usize>,
    pub notes: Vec<String>,
}

fn natural_key(p: &Path) -> (String, u64) {
    let name = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_owned();
    let digits: String = name
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let prefix = name[..name.len() - digits.len()].to_owned();
    (prefix, digits.parse().unwrap_or(0))
}

/// # Errors
///
/// Returns an error if the directory cannot be opened.
pub fn list_pngs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("{}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
        .collect();
    files.sort_by(|a, b| natural_key(a).cmp(&natural_key(b)).then(Ordering::Equal));
    Ok(files)
}

/// # Errors
///
/// Returns an error if fewer than three PNGs are found, a PNG cannot be read, or frame dimensions differ.
pub fn analyze_frames(dir: &Path, threshold: u8) -> Result<Frames> {
    let files = list_pngs(dir)?;
    if files.len() < MIN_FRAMES {
        bail!("need at least {MIN_FRAMES} PNG frames");
    }
    let images = files.iter().map(|p| load(p)).collect::<Result<Vec<_>>>()?;
    analyze_images(&images, threshold)
}

/// # Errors
///
/// Returns an error if there are fewer than three frames or their dimensions differ.
pub fn analyze_images(images: &[Image], threshold: u8) -> Result<Frames> {
    let [first, _, .., last] = images else {
        bail!("need at least {MIN_FRAMES} frames");
    };
    let mut ratios = Vec::new();
    for pair in images.windows(2) {
        let (d, _) = diff_images(&pair[0], &pair[1], threshold);
        if d.dimension_mismatch {
            bail!("frames have different dimensions");
        }
        ratios.push(d.changed_ratio);
    }
    let eps = STILL_FRAME_RATIO;
    let first_change = ratios.iter().position(|&r| r > eps).map(|i| i + 1);
    let settled_at = ratios.iter().rposition(|&r| r > eps).map(|i| i + 1);
    let total = diff_images(first, last, threshold).0.changed_ratio;
    let mut stalls = Vec::new();
    let mut jumps = Vec::new();
    let mut notes = Vec::new();
    if let (Some(f), Some(s)) = (first_change, settled_at) {
        let active: Vec<f64> = (f..=s).map(|frame| ratios[frame - 1]).collect();
        for frame in f..=s {
            if ratios[frame - 1] <= eps && frame != f && frame != s {
                stalls.push(frame);
            }
        }
        let mut nz: Vec<f64> = active.iter().copied().filter(|&r| r > eps).collect();
        nz.sort_by(f64::total_cmp);
        let median = nz.get(nz.len() / 2).copied().unwrap_or(0.0);
        for frame in f..=s {
            let r = ratios[frame - 1];
            if r > JUMP_MIN_RATIO && r > JUMP_MEDIAN_FACTOR * median {
                jumps.push(frame);
            }
        }
        if s - f + 1 < MIN_FRAMES {
            notes.push(format!(
                "the change spans fewer than {MIN_FRAMES} frames: it is effectively instant, not a tween"
            ));
        }
    } else {
        notes.push(
            "no frame changed: the animation did not run, or the capture missed it".to_owned(),
        );
    }
    if first_change.is_some() && total <= eps {
        notes.push("first and last frames match: the change returned to its start (check the capture window)".to_owned());
    }
    Ok(Frames {
        frame_count: images.len(),
        step_changed_ratio: ratios,
        total_changed_ratio: total,
        first_change,
        settled_at,
        stalls,
        jumps,
        notes,
    })
}
