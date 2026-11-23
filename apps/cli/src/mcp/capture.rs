//! Captures, evidence and probes.

use super::bundle::import_bundle;
use super::call::{ToolCall, cli, manifest, text};
use crate::{scenario, service};
use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Largest artifact returned inline to an agent.
const MAX_INLINE_ARTIFACT_BYTES: u64 = 8 * 1024 * 1024;

impl ToolCall<'_> {
    pub(super) fn evidence_import(&self) -> Result<Value> {
        let arg = |key: &str| self.arg(key);
        let output: String = import_bundle(std::path::Path::new(arg("path")?))?.to_string();
        Ok(text(&output))
    }

    pub(super) fn capture_run(&self) -> Result<Value> {
        let args = self.args;
        let output: String = {
            let mut input: scenario::Scenario = serde_json::from_value(args["scenario"].clone())?;
            let (id, label) = service::detect_project()?;
            input.set_project(id, label);
            let out = service::output()?;
            let module = std::env::var_os("RUPICOLA_PLAYWRIGHT_MODULE").map(PathBuf::from);
            let capture = scenario::web(
                input,
                &out,
                module.as_deref(),
                rupicola_core::visual::DEFAULT_DIFF_THRESHOLD,
                args["video"].as_bool().unwrap_or(false),
            )?;
            json!({"run":out.file_name().and_then(|name| name.to_str()), "manifest":capture})
                .to_string()
        };
        Ok(text(&output))
    }

    pub(super) fn evidence_list() -> Result<Value> {
        let output: String = {
            let dir = service::data()?.join("captures");
            let mut runs = Vec::new();
            if dir.exists() {
                for entry in std::fs::read_dir(dir)?.flatten() {
                    if let Some(id) = entry.file_name().to_str()
                        && let Ok((_, value)) = manifest(id)
                    {
                        runs.push(json!({"run":id,"scenario":value["scenario"],"status":value["status"],"inspection":value["inspection"]}));
                    }
                }
            }
            json!(runs).to_string()
        };
        Ok(text(&output))
    }

    pub(super) fn capture_review(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let (path, _) = manifest(arg("run")?)?;
            scenario::review(
                &path,
                arg("note")?.into(),
                args["artifact"].as_str().map(PathBuf::from),
            )?;
            "Inspection recorded".into()
        };
        Ok(text(&output))
    }

    pub(super) fn artifact_read(&self) -> Result<Value> {
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let (path, value) = manifest(arg("run")?)?;
            let artifact = arg("artifact")?;
            let listed = value["checkpoints"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|x| x["path"] == artifact)
                || value["clips"].as_array().into_iter().flatten().any(|x| {
                    ["analysis_path", "sheet_path"]
                        .iter()
                        .any(|key| x[key] == artifact)
                        || x["frames"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|frame| frame["path"] == artifact)
                });
            if !listed {
                bail!("Artifact is absent from manifest");
            }
            let relative = std::path::Path::new(artifact);
            if relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                bail!("Invalid artifact path");
            }
            let root = path.parent().unwrap().canonicalize()?;
            let file = root.join(relative).canonicalize()?;
            if !file.starts_with(root)
                || std::fs::metadata(&file)?.len() > MAX_INLINE_ARTIFACT_BYTES
            {
                bail!("Artifact outside run or larger than {MAX_INLINE_ARTIFACT_BYTES} bytes");
            }
            let bytes = std::fs::read(file)?;
            if matches!(artifact.rsplit_once('.'), Some((_, "png"))) {
                return Ok(
                    json!({"content":[{"type":"image","mimeType":"image/png","data":STANDARD.encode(&bytes)}]}),
                );
            }
            String::from_utf8(bytes)?
        };
        Ok(text(&output))
    }

    pub(super) fn probe_run(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let mut command = vec![
                "probe".into(),
                "--json".into(),
                format!("--url={}", arg("url")?),
            ];
            if let Some(m) = args["mode"].as_str() {
                command.push(format!("--mode={m}"));
            }
            if let Some(p) = args["pages"].as_str() {
                command.push(format!("--pages={p}"));
            }
            if let Some(v) = args["viewports"].as_str() {
                command.push(format!("--viewports={v}"));
            }
            if let Some(t) = args["themes"].as_str() {
                command.push(format!("--themes={t}"));
            }
            if let Some(mp) = args["max_pages"].as_u64() {
                command.push(format!("--max-pages={mp}"));
            }
            if args["video"].as_bool().unwrap_or(false) {
                command.push("--video".into());
            }
            cli(&command)?
        };
        Ok(text(&output))
    }
}
