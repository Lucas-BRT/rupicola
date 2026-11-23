//! Dispatching one `tools/call` request to its tool.

use super::preview::Preview;
use crate::{scenario, service};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Command};

pub(super) fn cli(args: &[String]) -> Result<String> {
    let result = Command::new(std::env::current_exe()?).args(args).output()?;
    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).trim().into())
    } else {
        bail!("{}", String::from_utf8_lossy(&result.stderr).trim());
    }
}

pub(super) fn manifest(run: &str) -> Result<(PathBuf, Value)> {
    if !scenario::identifier(run) {
        bail!("Invalid run ID");
    }
    let path = service::data()?
        .join("captures")
        .join(run)
        .join("manifest.json");
    let value: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    if value["project"]["id"] != service::detect_project()?.0 {
        bail!("Capture belongs to another project");
    }
    Ok((path, value))
}

/// Wraps text in the MCP tool-result envelope.
pub(super) fn text(output: &str) -> Value {
    json!({"content":[{"type":"text","text":output}]})
}

/// One `tools/call` request: the tool's name and its JSON arguments. Each tool is
/// a method, so the dispatcher stays a plain table.
pub(super) struct ToolCall<'a> {
    pub(super) name: &'a str,
    pub(super) args: &'a Value,
}

impl ToolCall<'_> {
    /// A required string argument.
    pub(super) fn arg(&self, key: &str) -> Result<&str> {
        self.args[key]
            .as_str()
            .ok_or_else(|| anyhow!("Expected string: {key}"))
    }

    pub(super) fn run(&self, preview: &mut Preview) -> Result<Value> {
        match self.name {
            "project_context" => Self::project_context(),
            "service_health" => Self::service_health(),
            "ui_scan" | "ui_discover" | "skill_check" => self.ui_scan(),
            "design_record" => self.design_record(),
            "preview_start" => self.preview_start(preview),
            "preview_status" => Self::preview_status(preview),
            "preview_stop" => Ok(Self::preview_stop(preview)),
            "evidence_import" => self.evidence_import(),
            "capture_run" => self.capture_run(),
            "evidence_list" => Self::evidence_list(),
            "capture_review" => self.capture_review(),
            "artifact_read" => self.artifact_read(),
            "propose_list" => self.propose_list(),
            "propose_get" => self.propose_get(),
            "propose_enrich" => self.propose_enrich(),
            "propose_await" => self.propose_await(),
            "problem_list" => self.problem_list(),
            "problem_add" => self.problem_add(),
            "problem_triage" => self.problem_triage(),
            "problem_progress" => self.problem_progress(),
            "fix_list" => self.fix_list(),
            "fix_record" => self.fix_record(),
            "storage_status" => self.storage_status(),
            "storage_clean" => self.storage_clean(),
            "storage_offload" => self.storage_offload(),
            _ => bail!("Unknown tool"),
        }
    }
}

pub(super) fn tool(name: &str, args: &Value, preview: &mut Preview) -> Result<Value> {
    ToolCall { name, args }.run(preview)
}
