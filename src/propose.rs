use crate::{
    proposal_record::{
        self, Artifact, ArtifactKind, Attachment, AttachmentKind, Comment, Origin, Proposal, Status,
    },
    service,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(clap::Subcommand)]
pub enum Action {
    /// List proposals for the current or specified project.
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show a single proposal and its structured prompt.
    Get {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Create a new proposal (for agents or CLI workflows).
    Add {
        #[arg(value_enum)]
        kind: proposal_record::Kind,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        problem: String,
        #[arg(long, default_value = "")]
        rationale: String,
        #[arg(long, default_value = "")]
        prompt: String,
        #[arg(long)]
        surface: Option<String>,
        #[arg(long = "tactic")]
        tactics: Vec<String>,
        #[arg(long = "reference")]
        references: Vec<String>,
        #[arg(long)]
        html_file: Option<PathBuf>,
        #[arg(long)]
        image_file: Option<PathBuf>,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        project: Option<String>,
    },
    /// Enrich an existing proposal with structured extraction and/or rendered artifact.
    Enrich {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long = "palette")]
        palette: Vec<String>,
        #[arg(long)]
        layout: Option<String>,
        #[arg(long = "tactic")]
        tactics: Vec<String>,
        #[arg(long)]
        refined_prompt: Option<String>,
        #[arg(long = "requirement")]
        requirements: Vec<String>,
        #[arg(long)]
        html_file: Option<PathBuf>,
        #[arg(long)]
        image_file: Option<PathBuf>,
        #[arg(long)]
        viewport: Option<String>,
        #[arg(long)]
        stack: Option<String>,
    },
    /// Update proposal status or add a review comment.
    Review {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, value_enum)]
        status: Status,
        #[arg(long)]
        comment: Option<String>,
    },
}

fn now_ms() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    u64::try_from(elapsed.as_millis()).map_err(|e| e.to_string())
}

fn resolve_project(explicit: Option<String>) -> Result<(String, PathBuf), String> {
    let id = match explicit {
        Some(id) => {
            if !proposal_record::safe_id(&id) {
                return Err("Invalid project ID".into());
            }
            id
        }
        None => service::detect_project()?.0,
    };
    let root = service::data()?.join(proposal_record::DIRECTORY).join(&id);
    Ok((id, root))
}

fn copy_asset(
    src: &Path,
    assets_dir: &Path,
    target_name: &str,
    max_bytes: u64,
) -> Result<(), String> {
    let meta = fs::metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
    if !meta.is_file() || meta.len() > max_bytes {
        return Err("Asset must be a regular file within size limits".into());
    }
    proposal_record::private_dir(assets_dir).map_err(|e| e.to_string())?;
    fs::copy(src, assets_dir.join(target_name)).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run(action: Action) -> Result<(), String> {
    match action {
        Action::List { project, json } => {
            let (id, root) = resolve_project(project)?;
            let items = proposal_record::entries(&root).map_err(|e| e.to_string())?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "project": id,
                        "proposals": items,
                    }))
                    .map_err(|e| e.to_string())?
                );
            } else if items.is_empty() {
                println!("No proposals for {id}.");
            } else {
                for p in items {
                    println!(
                        "{}\t{}\t{}\t{}",
                        p.id,
                        p.kind.slug(),
                        p.status.slug(),
                        p.title
                    );
                }
            }
        }
        Action::Get { id, project, json } => {
            if !proposal_record::safe_id(&id) {
                return Err("Invalid proposal ID".into());
            }
            let (project_id, root) = resolve_project(project)?;
            let p = proposal_record::read_proposal(&root.join(format!("{id}.json")))
                .map_err(|e| e.to_string())?;
            let parent = p
                .parent_id
                .as_deref()
                .filter(|pid| proposal_record::safe_id(pid))
                .and_then(|pid| {
                    proposal_record::read_proposal(&root.join(format!("{pid}.json"))).ok()
                });
            let prompt = proposal_record::structured_prompt(&project_id, &p, parent.as_ref());
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "project": project_id,
                        "proposal": p,
                        "structured_prompt": prompt,
                        "assets_dir": root.join(&p.id).join("assets"),
                    }))
                    .map_err(|e| e.to_string())?
                );
            } else {
                println!("{} · {} · {}", p.id, p.kind.label(), p.status.label());
                println!("{}", p.title);
                println!("\n{prompt}");
            }
        }
        Action::Add {
            kind,
            title,
            problem,
            rationale,
            prompt,
            surface,
            tactics,
            references,
            html_file,
            image_file,
            parent,
            project,
        } => {
            let (project_id, root) = resolve_project(project)?;
            let ts = now_ms()?;
            let id = format!("{ts}-{}", std::process::id());
            let assets_dir = root.join(&id).join("assets");
            let mut artifact = None;
            let mut attachments = Vec::new();
            if let Some(html) = html_file {
                copy_asset(&html, &assets_dir, "preview.html", 2 * 1024 * 1024)?;
                artifact = Some(Artifact {
                    kind: ArtifactKind::Html,
                    entry: "preview.html".into(),
                    viewport: Some("1440x900".into()),
                    stack: Some("web".into()),
                });
            } else if let Some(img) = image_file {
                let ext = img
                    .extension()
                    .and_then(|e| e.to_str())
                    .filter(|e| matches!(*e, "png" | "jpg" | "jpeg" | "webp"))
                    .unwrap_or("png");
                let filename = format!("render.{ext}");
                copy_asset(&img, &assets_dir, &filename, 16 * 1024 * 1024)?;
                attachments.push(Attachment {
                    id: "render".into(),
                    filename: filename.clone(),
                    content_type: format!("image/{ext}"),
                    kind: AttachmentKind::Image,
                });
                artifact = Some(Artifact {
                    kind: ArtifactKind::Image,
                    entry: filename,
                    viewport: None,
                    stack: None,
                });
            }
            let p = Proposal {
                id: id.clone(),
                parent_id: parent,
                timestamp_ms: ts,
                author: Some("agent".into()),
                origin: Origin::Agent,
                kind,
                status: Status::Proposed,
                title,
                problem_solved: problem,
                rationale,
                prompt,
                surface,
                tactics,
                references,
                attachments,
                artifact,
                extracted: None,
                comments: Vec::new(),
                dispatched_at_ms: None,
            };
            proposal_record::write_proposal(&root, &p).map_err(|e| e.to_string())?;
            println!("Recorded proposal {id} in {project_id}");
        }
        Action::Enrich {
            id,
            project,
            summary,
            palette,
            layout,
            tactics,
            refined_prompt,
            requirements,
            html_file,
            image_file,
            viewport,
            stack,
        } => {
            if !proposal_record::safe_id(&id) {
                return Err("Invalid proposal ID".into());
            }
            let (project_id, root) = resolve_project(project)?;
            let path = root.join(format!("{id}.json"));
            let mut p = proposal_record::read_proposal(&path).map_err(|e| e.to_string())?;
            let assets_dir = root.join(&id).join("assets");
            if let Some(html) = html_file {
                copy_asset(&html, &assets_dir, "preview.html", 2 * 1024 * 1024)?;
                p.artifact = Some(Artifact {
                    kind: ArtifactKind::Html,
                    entry: "preview.html".into(),
                    viewport: viewport.or_else(|| Some("1440x900".into())),
                    stack: stack.or_else(|| Some("web".into())),
                });
            } else if let Some(img) = image_file {
                let ext = img
                    .extension()
                    .and_then(|e| e.to_str())
                    .filter(|e| matches!(*e, "png" | "jpg" | "jpeg" | "webp"))
                    .unwrap_or("png");
                let filename = format!("render.{ext}");
                copy_asset(&img, &assets_dir, &filename, 16 * 1024 * 1024)?;
                p.artifact = Some(Artifact {
                    kind: ArtifactKind::Image,
                    entry: filename,
                    viewport,
                    stack,
                });
            }
            let mut ext = p.extracted.take().unwrap_or_default();
            if summary.is_some() {
                ext.summary = summary;
            }
            if !palette.is_empty() {
                ext.palette_hex = palette;
            }
            if layout.is_some() {
                ext.layout_structure = layout;
            }
            if !tactics.is_empty() {
                ext.identified_tactics = tactics;
            }
            if refined_prompt.is_some() {
                ext.refined_prompt = refined_prompt;
            }
            if !requirements.is_empty() {
                ext.target_requirements = requirements;
            }
            p.extracted = Some(ext);
            p.status = Status::Proposed;
            proposal_record::write_proposal(&root, &p).map_err(|e| e.to_string())?;
            println!("Enriched proposal {id} in {project_id}");
        }
        Action::Review {
            id,
            project,
            status,
            comment,
        } => {
            if !proposal_record::safe_id(&id) {
                return Err("Invalid proposal ID".into());
            }
            let (project_id, root) = resolve_project(project)?;
            let path = root.join(format!("{id}.json"));
            let mut p = proposal_record::read_proposal(&path).map_err(|e| e.to_string())?;
            p.status = status;
            if let Some(body) = comment
                .map(|c| c.trim().to_owned())
                .filter(|c| !c.is_empty())
            {
                let ts = now_ms()?;
                p.comments.push(Comment {
                    id: format!("{ts}-{}", std::process::id()),
                    author: "cli".into(),
                    timestamp_ms: ts,
                    body,
                    action: Some(status.slug().into()),
                });
            }
            proposal_record::write_proposal(&root, &p).map_err(|e| e.to_string())?;
            println!("Updated proposal {id} in {project_id} -> {}", status.slug());
        }
    }
    Ok(())
}
