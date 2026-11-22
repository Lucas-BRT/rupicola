//! Visual proposals: listing, reading, enriching and awaiting dispatches.

use super::call::{ToolCall, text};
use crate::{proposal_record, service};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Waiting for a proposal dispatch: default and longest wait, and poll interval.
const DEFAULT_DISPATCH_WAIT_SECS: u64 = 25;
const MAX_DISPATCH_WAIT_SECS: u64 = 120;
const DISPATCH_POLL: std::time::Duration = std::time::Duration::from_millis(400);

impl ToolCall<'_> {
    pub(super) fn propose_list(&self) -> Result<Value> {
        let args = self.args;
        let output: String = {
            let project_id = match args["project"].as_str() {
                Some(id) if proposal_record::safe_id(id) => id.to_owned(),
                Some(_) => bail!("Invalid project ID"),
                None => service::detect_project()?.0,
            };
            let root = service::data()?
                .join(proposal_record::DIRECTORY)
                .join(&project_id);
            let mut items = proposal_record::entries(&root)?;
            if let Some(status) = args["status"].as_str() {
                items.retain(|p| p.status.slug() == status);
            }
            let dispatches = proposal_record::dispatches(&root)?;
            serde_json::to_string_pretty(&json!({
                "project": project_id,
                "proposals": items,
                "pending_dispatches": dispatches,
            }))?
        };
        Ok(text(&output))
    }

    pub(super) fn propose_get(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let id = arg("id")?;
            if !proposal_record::safe_id(id) {
                bail!("Invalid proposal ID");
            }
            let project_id = match args["project"].as_str() {
                Some(p) if proposal_record::safe_id(p) => p.to_owned(),
                Some(_) => bail!("Invalid project ID"),
                None => service::detect_project()?.0,
            };
            let root = service::data()?
                .join(proposal_record::DIRECTORY)
                .join(&project_id);
            let p = proposal_record::read_proposal(&root.join(format!("{id}.json")))?;
            let parent = p
                .parent_id
                .as_deref()
                .filter(|pid| proposal_record::safe_id(pid))
                .and_then(|pid| {
                    proposal_record::read_proposal(&root.join(format!("{pid}.json"))).ok()
                });
            let prompt = proposal_record::structured_prompt(&project_id, &p, parent.as_ref());
            serde_json::to_string_pretty(&json!({
                "project": project_id,
                "proposal": p,
                "structured_prompt": prompt,
                "assets_dir": root.join(&p.id).join("assets"),
            }))?
        };
        Ok(text(&output))
    }

    pub(super) fn propose_enrich(&self) -> Result<Value> {
        let args = self.args;
        let arg = |key: &str| self.arg(key);
        let output: String = {
            let id = arg("id")?;
            if !proposal_record::safe_id(id) {
                bail!("Invalid proposal ID");
            }
            let project_id = match args["project"].as_str() {
                Some(p) if proposal_record::safe_id(p) => p.to_owned(),
                Some(_) => bail!("Invalid project ID"),
                None => service::detect_project()?.0,
            };
            let root = service::data()?
                .join(proposal_record::DIRECTORY)
                .join(&project_id);
            let path = root.join(format!("{id}.json"));
            let mut p = proposal_record::read_proposal(&path)?;
            let assets_dir = root.join(id).join("assets");
            if let Some(html) = args["html_content"].as_str() {
                if html.len() as u64 > proposal_record::MAX_PREVIEW_HTML_BYTES {
                    bail!(
                        "html_content exceeds {} bytes",
                        proposal_record::MAX_PREVIEW_HTML_BYTES
                    );
                }
                proposal_record::private_dir(&assets_dir)?;
                std::fs::write(assets_dir.join("preview.html"), html)?;
                p.artifact = Some(proposal_record::Artifact {
                    kind: proposal_record::ArtifactKind::Html,
                    entry: "preview.html".into(),
                    viewport: args["viewport"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| Some("1440x900".into())),
                    stack: args["stack"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| Some("web".into())),
                });
            } else if let Some(img) = args["image_path"].as_str() {
                let src = PathBuf::from(img);
                let meta = std::fs::metadata(&src)?;
                if !meta.is_file() || meta.len() > proposal_record::MAX_PREVIEW_IMAGE_BYTES {
                    bail!(
                        "image_path must be a regular file of at most {} bytes",
                        proposal_record::MAX_PREVIEW_IMAGE_BYTES
                    );
                }
                let ext = src
                    .extension()
                    .and_then(|e| e.to_str())
                    .filter(|e| matches!(*e, "png" | "jpg" | "jpeg" | "webp"))
                    .unwrap_or("png");
                let entry = format!("render.{ext}");
                proposal_record::private_dir(&assets_dir)?;
                std::fs::copy(&src, assets_dir.join(&entry))?;
                p.artifact = Some(proposal_record::Artifact {
                    kind: proposal_record::ArtifactKind::Image,
                    entry,
                    viewport: args["viewport"].as_str().map(str::to_owned),
                    stack: args["stack"].as_str().map(str::to_owned),
                });
            }
            let mut ext = p.extracted.take().unwrap_or_default();
            if let Some(s) = args["summary"].as_str() {
                ext.summary = Some(s.to_owned());
            }
            if let Some(arr) = args["palette_hex"].as_array() {
                ext.palette_hex = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            if let Some(s) = args["layout_structure"].as_str() {
                ext.layout_structure = Some(s.to_owned());
            }
            if let Some(arr) = args["identified_tactics"].as_array() {
                ext.identified_tactics = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            if let Some(s) = args["refined_prompt"].as_str() {
                ext.refined_prompt = Some(s.to_owned());
            }
            if let Some(arr) = args["target_requirements"].as_array() {
                ext.target_requirements = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            p.extracted = Some(ext);
            p.status = proposal_record::Status::Proposed;
            proposal_record::write_proposal(&root, &p)?;
            format!("Enriched proposal {id}")
        };
        Ok(text(&output))
    }

    pub(super) fn propose_await(&self) -> Result<Value> {
        let args = self.args;
        let project_id = match args["project"].as_str() {
            Some(p) if proposal_record::safe_id(p) => p.to_owned(),
            Some(_) => bail!("Invalid project ID"),
            None => service::detect_project()?.0,
        };
        let timeout = args["timeout_secs"]
            .as_u64()
            .unwrap_or(DEFAULT_DISPATCH_WAIT_SECS)
            .min(MAX_DISPATCH_WAIT_SECS);
        let root = service::data()?
            .join(proposal_record::DIRECTORY)
            .join(&project_id);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
        loop {
            let mut list = proposal_record::dispatches(&root)?;
            if let Some(dispatch) = list.pop() {
                let _ = std::fs::remove_file(
                    root.join(proposal_record::DISPATCH_DIR)
                        .join(format!("{}.json", dispatch.id)),
                );
                let proposal = proposal_record::read_proposal(
                    &root.join(format!("{}.json", dispatch.proposal_id)),
                )
                .ok();
                return Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json!({
                            "dispatch": dispatch,
                            "proposal": proposal,
                            "assets_dir": root.join(&dispatch.proposal_id).join("assets"),
                        }))?
                    }]
                }));
            }
            if std::time::Instant::now() >= deadline {
                return Ok(json!({"content":[{"type":"text","text":"No pending dispatches"}]}));
            }
            std::thread::sleep(DISPATCH_POLL);
        }
    }
}
