use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::{fs, io, path::Path};

pub const DIRECTORY: &str = "proposals";
pub const MCP_SESSION_FILE: &str = "mcp-session.json";
pub const DISPATCH_DIR: &str = "dispatch";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Palette,
    Component,
    Page,
    Layout,
    Application,
    DesignSystem,
    Motion,
    Other,
}

impl Kind {
    #[allow(dead_code)]
    pub const ALL: &'static [Self] = &[
        Self::Palette,
        Self::Component,
        Self::Page,
        Self::Layout,
        Self::Application,
        Self::DesignSystem,
        Self::Motion,
        Self::Other,
    ];

    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Palette => "palette",
            Self::Component => "component",
            Self::Page => "page",
            Self::Layout => "layout",
            Self::Application => "application",
            Self::DesignSystem => "design-system",
            Self::Motion => "motion",
            Self::Other => "other",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Palette => "Palette",
            Self::Component => "Component",
            Self::Page => "Page",
            Self::Layout => "Layout",
            Self::Application => "Application",
            Self::DesignSystem => "Design system",
            Self::Motion => "Motion",
            Self::Other => "Other",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Draft,
    PendingExtraction,
    Proposed,
    Approved,
    Rejected,
    VariationRequested,
}

impl Status {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingExtraction => "pending-extraction",
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::VariationRequested => "variation-requested",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::PendingExtraction => "Pending extraction",
            Self::Proposed => "Proposed",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::VariationRequested => "Variation requested",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    User,
    Agent,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttachmentKind {
    Image,
    Video,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub kind: AttachmentKind,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Html,
    Image,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub entry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Extracted {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palette_hex: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_structure: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identified_tactics: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refined_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_requirements: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub timestamp_ms: u64,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Proposal {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub origin: Origin,
    pub kind: Kind,
    pub status: Status,
    pub title: String,
    pub problem_solved: String,
    pub rationale: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tactics: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted: Option<Extracted>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpSession {
    pub pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    pub started_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Dispatch {
    pub id: String,
    pub proposal_id: String,
    pub project_id: String,
    pub created_at_ms: u64,
    pub structured_prompt: String,
}

/// Largest HTML preview and image a proposal may carry.
pub const MAX_PREVIEW_HTML_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PREVIEW_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
/// Longest proposal id.
const MAX_ID_LEN: usize = 80;
/// Largest JSON file read for a proposal, the MCP session marker and a dispatch.
const MAX_PROPOSAL_BYTES: u64 = 1024 * 1024;
pub const MAX_SESSION_BYTES: u64 = 16 * 1024;
const MAX_DISPATCH_BYTES: u64 = 256 * 1024;

#[must_use]
pub fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// # Errors
///
/// Returns an error if the directory or one of its missing parents cannot be created.
pub fn private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// # Errors
///
/// Returns an error if the file cannot be accessed, is not a regular file, exceeds the size limit, or cannot be decoded.
pub fn read_json<T: serde::de::DeserializeOwned>(file: &Path, max_bytes: u64) -> io::Result<T> {
    let metadata = fs::symlink_metadata(file)?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "record must be a regular file within size limits",
        ));
    }
    serde_json::from_slice(&fs::read(file)?).map_err(io::Error::other)
}

/// # Errors
///
/// Returns an error if the parent directory or temporary file cannot be created, serialization fails, or the file cannot be written, synced, or renamed.
pub fn write_atomic(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    use io::Write;
    if let Some(parent) = path.parent() {
        private_dir(parent)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)?;
    let result = (|| {
        serde_json::to_writer_pretty(&mut file, value).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// # Errors
///
/// Returns an error if the record cannot be read or decoded, exceeds the size limit, or its ID does not match the file name.
pub fn read_proposal(file: &Path) -> io::Result<Proposal> {
    let p: Proposal = read_json(file, MAX_PROPOSAL_BYTES)?;
    if file.file_stem().and_then(|s| s.to_str()) != Some(p.id.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "proposal ID does not match its file",
        ));
    }
    Ok(p)
}

/// # Errors
///
/// Returns an error if the ID is invalid, serialization fails, or the record cannot be saved.
pub fn write_proposal(project_root: &Path, proposal: &Proposal) -> io::Result<()> {
    if !safe_id(&proposal.id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid proposal ID",
        ));
    }
    write_atomic(
        &project_root.join(format!("{}.json", proposal.id)),
        proposal,
    )
}

/// # Errors
///
/// Returns an error if the existing proposal directory cannot be opened or iterated.
pub fn entries(project_root: &Path) -> io::Result<Vec<Proposal>> {
    let mut out = Vec::new();
    let dir = match fs::read_dir(project_root) {
        Ok(dir) => dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(err) => return Err(err),
    };
    for entry in dir {
        let path = entry?.path();
        if path.extension() != Some(std::ffi::OsStr::new("json")) {
            continue;
        }
        if let Ok(proposal) = read_proposal(&path) {
            out.push(proposal);
        }
    }
    out.sort_by(|a, b| (b.timestamp_ms, &b.id).cmp(&(a.timestamp_ms, &a.id)));
    Ok(out)
}

#[allow(dead_code)]
#[must_use]
pub fn mcp_session_active(data_dir: &Path) -> Option<McpSession> {
    let session: McpSession =
        read_json(&data_dir.join(MCP_SESSION_FILE), MAX_SESSION_BYTES).ok()?;
    if session.pid == 0 {
        return None;
    }
    #[cfg(unix)]
    {
        if !Path::new(&format!("/proc/{}", session.pid)).exists() {
            return None;
        }
    }
    Some(session)
}

/// # Errors
///
/// Returns an error if the existing dispatch directory cannot be opened or iterated.
#[allow(dead_code)]
pub fn dispatches(project_root: &Path) -> io::Result<Vec<Dispatch>> {
    let mut out = Vec::new();
    let dir = match fs::read_dir(project_root.join(DISPATCH_DIR)) {
        Ok(dir) => dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(out),
        Err(err) => return Err(err),
    };
    for entry in dir {
        let path = entry?.path();
        if path.extension() != Some(std::ffi::OsStr::new("json")) {
            continue;
        }
        if let Ok(d) = read_json::<Dispatch>(&path, MAX_DISPATCH_BYTES) {
            out.push(d);
        }
    }
    out.sort_by(|a, b| (b.created_at_ms, &b.id).cmp(&(a.created_at_ms, &a.id)));
    Ok(out)
}

#[must_use]
pub fn structured_prompt(project_id: &str, p: &Proposal, parent: Option<&Proposal>) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "Processar proposta `{}` do projeto `{}`",
        p.id, project_id
    )
    .expect("writing to a String cannot fail");
    writeln!(
        out,
        "Tipo: {} | Status: {} | Superfície: {}",
        p.kind.label(),
        p.status.label(),
        p.surface.as_deref().unwrap_or("não especificada")
    )
    .expect("writing to a String cannot fail");
    writeln!(out, "Título: {}", p.title).expect("writing to a String cannot fail");
    if !p.problem_solved.is_empty() {
        writeln!(out, "Problema a resolver: {}", p.problem_solved)
            .expect("writing to a String cannot fail");
    }
    if !p.rationale.is_empty() {
        writeln!(out, "Justificativa: {}", p.rationale).expect("writing to a String cannot fail");
    }
    if !p.prompt.is_empty() {
        writeln!(out, "Prompt/Objetivo:\n{}", p.prompt).expect("writing to a String cannot fail");
    }
    if !p.tactics.is_empty() {
        writeln!(out, "Táticas aplicadas: {}", p.tactics.join(", "))
            .expect("writing to a String cannot fail");
    }
    if !p.references.is_empty() {
        writeln!(out, "Referências: {}", p.references.join(", "))
            .expect("writing to a String cannot fail");
    }
    if !p.attachments.is_empty() {
        let files: Vec<_> = p
            .attachments
            .iter()
            .map(|a| format!("{} ({})", a.filename, a.content_type))
            .collect();
        writeln!(out, "Anexos: {}", files.join(", ")).expect("writing to a String cannot fail");
    }
    if let Some(parent) = parent {
        writeln!(out, "Variação baseada em: {} ({})", parent.id, parent.title)
            .expect("writing to a String cannot fail");
    }
    if !p.comments.is_empty() {
        out.push_str("Comentários e observações:\n");
        for c in &p.comments {
            writeln!(
                out,
                "- [{}] {}: {}",
                c.author,
                c.action.as_deref().unwrap_or("comment"),
                c.body
            )
            .expect("writing to a String cannot fail");
        }
    }
    out.push_str(
        "Ação esperada via MCP: ler anexos/referências com `propose_get`, extrair tokens/paleta/táticas/requisitos estruturados, gerar artefato HTML (web) ou PNG hi-res e publicar com `propose_enrich`.",
    );
    out
}
