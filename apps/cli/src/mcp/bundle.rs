//! Importing a capture bundle from disk into the shared captures directory.

use crate::service;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Largest capture manifest read.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Most files, and most media bytes, in one published capture bundle.
const MAX_BUNDLE_ASSETS: usize = 256;
const MAX_BUNDLE_BYTES: u64 = 40 * 1024 * 1024;

pub(super) fn import_bundle(source: &std::path::Path) -> Result<Value> {
    use std::{fs, io::Read, path::Component};
    let read = |path: &std::path::Path, limit: u64| -> Result<Vec<u8>> {
        let file = fs::File::open(path)?;
        if !file.metadata()?.is_file() {
            bail!("Expected a regular file");
        }
        let mut bytes = Vec::new();
        file.take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            bail!("Bundle exceeds size limit");
        }
        Ok(bytes)
    };
    let root = source.canonicalize()?;
    let resolve = |name: &str| -> Result<PathBuf> {
        let relative = std::path::Path::new(name);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            bail!("Invalid bundle path");
        }
        let file = root.join(relative).canonicalize()?;
        if !file.starts_with(&root) || !file.is_file() {
            bail!("Bundle file is outside source or is not regular");
        }
        Ok(file)
    };
    let bytes = read(&resolve("manifest.json")?, MAX_MANIFEST_BYTES)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let manifest: rupicola_core::capture_manifest::Manifest =
        serde_json::from_slice(&bytes).context("Invalid capture manifest")?;
    if manifest.version != 1 || manifest.project.id != service::detect_project()?.0 {
        bail!("Expected a v1 capture belonging to this project");
    }
    let mut paths = std::collections::BTreeSet::new();
    for checkpoint in value["checkpoints"].as_array().into_iter().flatten() {
        if let Some(path) = checkpoint["path"].as_str() {
            paths.insert(path);
        }
    }
    for clip in value["clips"].as_array().into_iter().flatten() {
        for key in ["analysis_path", "sheet_path", "video_path"] {
            if let Some(path) = clip[key].as_str() {
                paths.insert(path);
            }
        }
        for frame in clip["frames"].as_array().into_iter().flatten() {
            if let Some(path) = frame["path"].as_str() {
                paths.insert(path);
            }
        }
    }
    if paths.len() > MAX_BUNDLE_ASSETS {
        bail!("Bundle exceeds {MAX_BUNDLE_ASSETS} assets");
    }
    let out = service::output()?;
    let staging = out.with_extension("import");
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    fs::create_dir_all(out.parent().unwrap())?;
    builder.create(&staging)?;
    let result = (|| -> Result<()> {
        let mut remaining = MAX_BUNDLE_BYTES;
        for path in paths {
            if !matches!(
                std::path::Path::new(path)
                    .extension()
                    .and_then(|v| v.to_str()),
                Some("png" | "json" | "mp4")
            ) {
                bail!("Unsupported bundle asset");
            }
            let asset = read(&resolve(path)?, remaining)?;
            remaining -= asset.len() as u64;
            let destination = staging.join(path);
            fs::create_dir_all(destination.parent().unwrap())?;
            fs::write(destination, asset)?;
        }
        fs::write(staging.join("manifest.json"), bytes)?;
        fs::rename(&staging, &out)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result?;
    Ok(json!({"run":out.file_name().and_then(|name| name.to_str()),"manifest":value}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_import_rejects_missing_panel_metadata() {
        /// Any valid millisecond timestamp will do for the fixture manifest.
        const FIXTURE_CREATED_AT_MS: i64 = 1_795_182_148_405;
        let source = tempfile::tempdir().unwrap();
        let path = source.path().join("manifest.json");
        let valid = json!({
            "version": 1,
            "project": {"id": "example", "label": "Example"},
            "scenario": "Settings review",
            "created_at_ms": FIXTURE_CREATED_AT_MS,
            "environment": {"renderer": "gpui"},
            "status": "complete",
            "inspection": "pending",
            "checkpoints": [],
            "clips": []
        });
        assert!(
            serde_json::from_value::<rupicola_core::capture_manifest::Manifest>(valid.clone())
                .is_ok()
        );
        for field in ["created_at_ms", "environment"] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            std::fs::write(&path, serde_json::to_vec(&missing).unwrap()).unwrap();
            let error = format!("{:#}", import_bundle(source.path()).unwrap_err());
            assert!(error.starts_with("Invalid capture manifest:"), "{error}");
            assert!(error.contains(field), "{error}");
        }
    }
}
