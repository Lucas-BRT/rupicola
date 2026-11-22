use crate::{
    design_record::{self, Entry, Kind, Project, Status},
    service,
};
use anyhow::{Context, Result, anyhow, bail};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Largest capture manifest read when linking evidence.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
/// Longest design-record title, note, surface name and reference URL.
const MAX_TITLE_LEN: usize = 200;
const MAX_NOTE_LEN: usize = 16_000;
const MAX_SURFACE_LEN: usize = 200;
const MAX_URL_LEN: usize = 2048;

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

fn evidence(path: &Path, root: &Path, project: &str) -> Result<String> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Expected a capture manifest path"))?;
    let captures = fs::canonicalize(root.join("captures"))?;
    let parent = fs::canonicalize(parent)?;
    if parent.parent() != Some(&captures)
        || path.file_name().and_then(|name| name.to_str()) != Some("manifest.json")
    {
        bail!("Evidence must be a manifest in the shared panel captures directory");
    }
    let manifest = parent.join("manifest.json");
    let metadata = fs::symlink_metadata(&manifest)?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        bail!("Invalid capture manifest");
    }
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(manifest)?)?;
    if manifest.pointer("/project/id").and_then(|id| id.as_str()) != Some(project) {
        bail!("Evidence belongs to another project");
    }
    parent
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Invalid capture directory name"))
}

#[expect(
    clippy::too_many_lines,
    reason = "the CLI dispatcher keeps the design subcommands together"
)]
pub fn run(action: Action) -> Result<()> {
    let (id, label) = service::detect_project()?;
    if !rupicola_core::scenario::identifier(&id) {
        bail!("Invalid project ID");
    }
    let data = service::data()?;
    let root = data.join(design_record::DIRECTORY).join(&id);
    let project = Project { id, label };
    private_dir(&root).with_context(|| format!("{}", root.display()))?;
    match design_record::project(&root) {
        Ok(saved) if saved.id == project.id => {}
        Ok(_) => bail!("Design project ID does not match its directory"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match write_new(&root.join("project.json"), &project) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).with_context(|| format!("{}", root.display())),
            }
        }
        Err(error) => return Err(error).with_context(|| format!("{}", root.display())),
    }
    let entries = design_record::entries(&root).with_context(|| format!("{}", root.display()))?;
    match action {
        Action::Open { json } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"project":project, "entries":entries})
                    )?
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
                || title.len() > MAX_TITLE_LEN
                || note.trim().is_empty()
                || note.len() > MAX_NOTE_LEN
                || surface
                    .as_ref()
                    .is_some_and(|text| text.len() > MAX_SURFACE_LEN)
                || reference.as_ref().is_some_and(|url| {
                    url.len() > MAX_URL_LEN
                        || url.chars().any(char::is_whitespace)
                        || !(url.starts_with("https://") || url.starts_with("http://"))
                })
            {
                bail!(
                    "Provide a title and note within the size limits and an HTTP(S) reference if needed"
                );
            }
            if supersedes.as_ref().is_some_and(|id| {
                !entries
                    .iter()
                    .any(|entry| &entry.id == id && entry.kind == kind)
            }) {
                bail!(
                    "The entry being superseded must exist in this project and have the same kind"
                );
            }
            let evidence = path
                .as_deref()
                .map(|path| evidence(path, &data, &project.id))
                .transpose()?;
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
            let id = format!("{}-{}", now.as_nanos(), std::process::id());
            let entry = Entry {
                id,
                timestamp_ms: u64::try_from(now.as_millis()).unwrap_or(u64::MAX),
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
            write_new(&root.join(format!("{}.json", entry.id)), &entry)
                .with_context(|| format!("{}", root.display()))?;
            println!("Recorded {} in {}", entry.id, project.label);
        }
    }
    Ok(())
}
