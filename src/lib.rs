//! Static, offline helpers for the Ferris Atelier skill.
//! No inspected project code is compiled or executed by these functions.
pub mod capture;
pub mod check;
pub mod design_record;
pub mod discover;
pub mod evals;
pub mod fix_record;
pub mod problem_record;
pub mod proposal_record;
pub mod scan;
pub mod scenario;
pub mod storage;
pub mod visual;

use serde::Serialize;
use std::path::{Path, PathBuf};

pub const FRAMEWORKS: &[&str] = &[
    "libcosmic",
    "gpui",
    "iced",
    "topcoat",
    "leptos",
    "yew",
    "tauri",
    "dioxus",
    "egui",
    "slint",
    "gtk4",
    "libadwaita",
    "relm4",
];
pub const TOPICS: &[&str] = &[
    "perception",
    "layout",
    "typography",
    "color",
    "iconography",
    "navigation",
    "input",
    "feedback",
    "motion",
    "data",
    "content",
    "accessibility",
    "localization",
];

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub message: String,
}

/// Deterministic traversal, excluding symlinks (including directory cycles).
/// An explicitly supplied symlink root is canonicalized; nested symlinks are skipped.
pub fn files(root: &Path, include_tests: bool) -> Result<Vec<PathBuf>, String> {
    fn visit(path: &Path, tests: bool, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let meta =
            std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        if meta.is_file() {
            out.push(path.to_path_buf());
            return Ok(());
        }
        if !meta.is_dir() {
            return Ok(());
        }
        let mut entries = std::fs::read_dir(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.')
                || matches!(
                    name.as_ref(),
                    "target" | "node_modules" | "vendor" | "dist" | "build" | "generated"
                )
            {
                continue;
            }
            if !tests && matches!(name.as_ref(), "tests" | "fixtures" | "examples" | "benches") {
                continue;
            }
            visit(&entry.path(), tests, out)?;
        }
        Ok(())
    }
    let root = root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", root.display()))?;
    let mut out = Vec::new();
    visit(&root, include_tests, &mut out)?;
    Ok(out)
}

pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}
