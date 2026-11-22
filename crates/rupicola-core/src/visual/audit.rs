//! Accessibility and layout audit of a JSON UI tree.

use super::Rgb;
use serde::{Deserialize, Serialize};

/// UI audit thresholds (WCAG 2.2): smallest target, default font size, large
/// text sizes, required contrast ratios, the tolerance for comparing them and
/// for a rectangle poking outside the window, how many interactive nodes are
/// compared for overlap, the overlap share that counts, and the gap range that
/// looks like an accidental misalignment.
const MIN_TARGET_PX: f32 = 24.0;
const DEFAULT_FONT_PX: f32 = 14.0;
const LARGE_TEXT_PX: f32 = 24.0;
const LARGE_BOLD_TEXT_PX: f32 = 18.66;
const CONTRAST_LARGE: f64 = 3.0;
const CONTRAST_NORMAL: f64 = 4.5;
const CONTRAST_EPSILON: f64 = 1e-9;
const OFFSCREEN_TOLERANCE_PX: f32 = 0.5;
const MAX_OVERLAP_NODES: usize = 2000;
const OVERLAP_SHARE: f32 = 0.25;
const NEAR_ALIGNMENT_MIN_PX: f32 = 0.01;
const NEAR_ALIGNMENT_MAX_PX: f32 = 3.0;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    pub window: WindowSize,
    pub nodes: Vec<Node>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowSize {
    pub w: f32,
    pub h: f32,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub role: String,
    #[serde(default)]
    pub name: Option<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default)]
    pub fg: Option<String>,
    #[serde(default)]
    pub bg: Option<String>,
    #[serde(default)]
    pub font_px: Option<f32>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub clipped: bool,
    #[serde(default)]
    pub parent: Option<String>,
    /// "text" (default), "icon" or "boundary": non-text parts need 3:1.
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuditFinding {
    pub rule: &'static str,
    pub node: String,
    pub message: String,
}

pub const AUDIT_RULES: &[(&str, &str)] = &[
    (
        "target-size",
        "An interactive node is smaller than 24 x 24 px (WCAG 2.5.8, unless spacing applies).",
    ),
    (
        "missing-name",
        "An interactive node or an image has no accessible name.",
    ),
    (
        "contrast",
        "Text needs 4.5:1 (3:1 for large text), icons and boundaries 3:1.",
    ),
    ("clipped", "A node reports clipped content."),
    ("offscreen", "A node extends outside the window."),
    (
        "overlap",
        "Two interactive nodes overlap by more than a quarter of the smaller one.",
    ),
    (
        "near-alignment",
        "Siblings have left edges that differ by 1 to 3 px, which reads as a misalignment.",
    ),
];

/// Why a node's text or icon contrast falls short of WCAG 2.2, if it does.
fn contrast_shortfall(n: &Node) -> Option<String> {
    let fg: Rgb = n.fg.as_deref()?.parse().ok()?;
    let bg: Rgb = n.bg.as_deref()?.parse().ok()?;
    let non_text = matches!(n.kind.as_deref(), Some("icon" | "boundary"));
    let px = n.font_px.unwrap_or(DEFAULT_FONT_PX);
    let large = px >= LARGE_TEXT_PX || (n.bold && px >= LARGE_BOLD_TEXT_PX);
    let need = if non_text || large {
        CONTRAST_LARGE
    } else {
        CONTRAST_NORMAL
    };
    let got = fg.contrast(bg);
    (got + CONTRAST_EPSILON < need).then(|| format!("{got:.2}:1, needs {need}:1"))
}

#[must_use]
pub fn audit(tree: &Tree) -> Vec<AuditFinding> {
    let mut out = Vec::new();
    let mut add = |rule: &'static str, node: &str, message: String| {
        out.push(AuditFinding {
            rule,
            node: node.to_owned(),
            message,
        });
    };
    for n in &tree.nodes {
        if n.interactive && (n.w < MIN_TARGET_PX || n.h < MIN_TARGET_PX) {
            add(
                "target-size",
                &n.id,
                format!(
                    "{} x {} px is below {MIN_TARGET_PX} x {MIN_TARGET_PX} px",
                    n.w, n.h
                ),
            );
        }
        if (n.interactive || matches!(n.role.as_str(), "image" | "icon"))
            && n.name.as_deref().is_none_or(|s| s.trim().is_empty())
        {
            add(
                "missing-name",
                &n.id,
                format!("{} has no accessible name", n.role),
            );
        }
        if let Some(message) = contrast_shortfall(n) {
            add("contrast", &n.id, message);
        }
        if n.clipped {
            add("clipped", &n.id, "content is clipped".to_owned());
        }
        if n.x < -OFFSCREEN_TOLERANCE_PX
            || n.y < -OFFSCREEN_TOLERANCE_PX
            || n.x + n.w > tree.window.w + OFFSCREEN_TOLERANCE_PX
            || n.y + n.h > tree.window.h + OFFSCREEN_TOLERANCE_PX
        {
            add(
                "offscreen",
                &n.id,
                format!(
                    "rectangle {},{} {}x{} leaves the {}x{} window",
                    n.x, n.y, n.w, n.h, tree.window.w, tree.window.h
                ),
            );
        }
    }
    let interactive: Vec<&Node> = tree
        .nodes
        .iter()
        .filter(|n| n.interactive)
        .take(MAX_OVERLAP_NODES)
        .collect();
    for (i, a) in interactive.iter().enumerate() {
        for b in &interactive[i + 1..] {
            let ix = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
            let iy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
            if ix > 0.0 && iy > 0.0 {
                let smaller = (a.w * a.h).min(b.w * b.h).max(1.0);
                if ix * iy / smaller > OVERLAP_SHARE {
                    add("overlap", &a.id, format!("overlaps {}", b.id));
                }
            }
        }
    }
    let mut parents: std::collections::BTreeMap<&str, Vec<&Node>> =
        std::collections::BTreeMap::new();
    for n in &tree.nodes {
        if let Some(p) = n.parent.as_deref() {
            parents.entry(p).or_default().push(n);
        }
    }
    for kids in parents.values_mut() {
        kids.sort_by(|a, b| a.x.total_cmp(&b.x));
        for pair in kids.windows(2) {
            let d = pair[1].x - pair[0].x;
            if d > NEAR_ALIGNMENT_MIN_PX && d <= NEAR_ALIGNMENT_MAX_PX {
                add(
                    "near-alignment",
                    &pair[1].id,
                    format!("left edge is {d:.1} px from {}", pair[0].id),
                );
            }
        }
    }
    out
}
