use crate::{
    design_record::{self, Entry, Kind, Project, Status},
    service,
};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(clap::Subcommand)]
pub enum Action {
    /// Register this project and read its current design context.
    Open {
        /// Return all records, including superseded entries, as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Record a brief, art direction, tactic, or decision.
    Add {
        #[arg(value_enum)]
        kind: Kind,
        #[arg(long, value_enum, default_value = "adopted")]
        status: Status,
        #[arg(long)]
        title: String,
        /// Record the decision and rationale.
        #[arg(long)]
        note: String,
        #[arg(long)]
        surface: Option<String>,
        /// ID of an older entry of the same kind.
        #[arg(long)]
        supersedes: Option<String>,
        /// HTTP(S) design reference.
        #[arg(long)]
        reference: Option<String>,
        /// Path to a shared-panel capture manifest of this project.
        #[arg(long)]
        evidence: Option<PathBuf>,
    },
}

fn private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn write_new(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)?;
    let result = (|| {
        serde_json::to_writer_pretty(&mut file, value).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::hard_link(&tmp, path)
    })();
    let _ = fs::remove_file(tmp);
    result
}

fn evidence(path: &Path, root: &Path, project: &str) -> Result<String, String> {
    let parent = path.parent().ok_or("Expected a capture manifest path")?;
    let captures = fs::canonicalize(root.join("captures")).map_err(|e| e.to_string())?;
    let parent = fs::canonicalize(parent).map_err(|e| e.to_string())?;
    if parent.parent() != Some(&captures)
        || path.file_name().and_then(|name| name.to_str()) != Some("manifest.json")
    {
        return Err("Evidence must be a manifest in the shared panel captures directory".into());
    }
    let manifest = parent.join("manifest.json");
    let metadata = fs::symlink_metadata(&manifest).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err("Invalid capture manifest".into());
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if manifest.pointer("/project/id").and_then(|id| id.as_str()) != Some(project) {
        return Err("Evidence belongs to another project".into());
    }
    parent
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or("Invalid capture directory name".into())
}

pub fn run(action: Action) -> Result<(), String> {
    let (id, label) = service::detect_project()?;
    if !ferris_atelier::scenario::identifier(&id) {
        return Err("Invalid project ID".into());
    }
    let data = service::data()?;
    let root = data.join(design_record::DIRECTORY).join(&id);
    let project = Project { id, label };
    let wrap = |e| format!("{}: {e}", root.display());
    private_dir(&root).map_err(wrap)?;
    match design_record::project(&root) {
        Ok(saved) if saved.id == project.id => {}
        Ok(_) => return Err("Design project ID does not match its directory".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match write_new(&root.join("project.json"), &project) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(wrap(error)),
            }
        }
        Err(error) => return Err(wrap(error)),
    }
    let entries = design_record::entries(&root).map_err(wrap)?;
    match action {
        Action::Open { json } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"project":project, "entries":entries})
                    )
                    .map_err(|e| e.to_string())?
                );
            } else {
                println!("{} ({})", project.label, project.id);
                let superseded = design_record::superseded(&entries);
                for entry in entries
                    .iter()
                    .filter(|entry| !superseded.contains(entry.id.as_str()))
                {
                    println!(
                        "\n[{} · {}] {} ({})",
                        entry.kind.label(),
                        entry.status.label(),
                        entry.title,
                        entry.id
                    );
                    if let Some(surface) = &entry.surface {
                        println!("Surface: {surface}");
                    }
                    println!("{}", entry.note);
                    if let Some(reference) = &entry.reference {
                        println!("Reference: {reference}");
                    }
                    if let Some(evidence) = &entry.evidence {
                        println!("Evidence: {evidence}");
                    }
                }
                if entries.is_empty() {
                    println!("No design records yet.");
                }
            }
        }
        Action::Add {
            kind,
            status,
            title,
            note,
            surface,
            supersedes,
            reference,
            evidence: path,
        } => {
            if title.trim().is_empty()
                || title.len() > 200
                || note.trim().is_empty()
                || note.len() > 16000
                || surface.as_ref().is_some_and(|text| text.len() > 200)
                || reference.as_ref().is_some_and(|url| {
                    url.len() > 2048
                        || url.chars().any(char::is_whitespace)
                        || !(url.starts_with("https://") || url.starts_with("http://"))
                })
            {
                return Err("Provide a title and note within the size limits and an HTTP(S) reference if needed".into());
            }
            if supersedes.as_ref().is_some_and(|id| {
                !entries
                    .iter()
                    .any(|entry| &entry.id == id && entry.kind == kind)
            }) {
                return Err(
                    "The entry being superseded must exist in this project and have the same kind"
                        .into(),
                );
            }
            let evidence = path
                .as_deref()
                .map(|path| evidence(path, &data, &project.id))
                .transpose()?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?;
            let id = format!("{}-{}", now.as_nanos(), std::process::id());
            let entry = Entry {
                id,
                timestamp_ms: now.as_millis() as u64,
                kind,
                status,
                title,
                note,
                surface,
                supersedes,
                reference,
                evidence,
                author: None,
            };
            write_new(&root.join(format!("{}.json", entry.id)), &entry).map_err(wrap)?;
            println!("Recorded {} in {}", entry.id, project.label);
        }
    }
    Ok(())
}
