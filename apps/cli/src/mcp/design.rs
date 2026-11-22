//! Project context, UI scans and design records.

use super::call::{ToolCall, cli, manifest, text};
use anyhow::Result;
use serde_json::Value;

impl ToolCall<'_> {
    pub(super) fn project_context() -> Result<Value> {
        let output: String = cli(&["design".into(), "open".into(), "--json".into()])?;
        Ok(text(&output))
    }

    pub(super) fn service_health() -> Result<Value> {
        let output: String = cli(&["doctor".into()])?;
        Ok(text(&output))
    }

    pub(super) fn ui_scan(&self) -> Result<Value> {
        let args = self.args;
        let name = self.name;
        let output: String = {
            let command = match name {
                "ui_scan" => "scan",
                "ui_discover" => "discover",
                _ => "check",
            };
            cli(&[
                command.into(),
                "--json".into(),
                "--".into(),
                args["path"].as_str().unwrap_or(".").into(),
            ])?
        };
        Ok(text(&output))
    }

    pub(super) fn design_record(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let mut command = vec!["design".into(), "add".into()];
            for key in [
                "title",
                "note",
                "status",
                "surface",
                "reference",
                "supersedes",
            ] {
                if let Some(value) = args[key].as_str() {
                    command.push(format!("--{key}={value}"));
                }
            }
            if let Some(run) = args["run"].as_str() {
                command.push(format!("--evidence={}", manifest(run)?.0.display()));
            }
            command.extend(["--".into(), arg("kind")?.into()]);
            cli(&command)?
        };
        Ok(text(&output))
    }
}
