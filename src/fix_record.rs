#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const DIRECTORY: &str = "fixes";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum FixStatus {
    PendingReview,
    Approved,
    ChangesRequested,
    Superseded,
}

impl FixStatus {
    pub const ALL: &'static [Self] = &[
        Self::PendingReview,
        Self::Approved,
        Self::ChangesRequested,
        Self::Superseded,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::PendingReview => "pending-review",
            Self::Approved => "approved",
            Self::ChangesRequested => "changes-requested",
            Self::Superseded => "superseded",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PendingReview => "Pending review",
            Self::Approved => "Approved",
            Self::ChangesRequested => "Changes requested",
            Self::Superseded => "Superseded",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "pending-review" => Some(Self::PendingReview),
            "approved" => Some(Self::Approved),
            "changes-requested" => Some(Self::ChangesRequested),
            "superseded" => Some(Self::Superseded),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ComparisonPair {
    pub label: String,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub before_url: String,
    pub after_url: String,
    #[serde(default)]
    pub diff_url: Option<String>,
    #[serde(default)]
    pub changed_ratio: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixStep {
    pub index: usize,
    pub title: String,
    pub detail: String,
    pub verification: String,
    #[serde(default)]
    pub evidence_url: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixReview {
    pub author: String,
    pub action: String,
    pub comment: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FixRecord {
    pub id: String,
    pub project_id: String,
    #[serde(default)]
    pub problem_ids: Vec<String>,
    pub title: String,
    pub summary: String,
    pub status: FixStatus,
    #[serde(default)]
    pub tactics: Vec<String>,
    #[serde(default)]
    pub modified_files: Vec<String>,
    #[serde(default)]
    pub comparisons: Vec<ComparisonPair>,
    #[serde(default)]
    pub steps: Vec<FixStep>,
    #[serde(default)]
    pub reviews: Vec<FixReview>,
    pub author: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

pub fn root(data_dir: &Path, project: &str) -> PathBuf {
    data_dir.join(DIRECTORY).join(project)
}

pub fn next_id() -> (String, u64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let ms = now.as_millis() as u64;
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) % 1000;
    (format!("{}-{}{:03}", ms, std::process::id(), seq), ms)
}

pub fn entries(data_dir: &Path, project: &str) -> io::Result<Vec<FixRecord>> {
    if !valid_id(project) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid project id",
        ));
    }
    let dir = root(data_dir, project);
    let read = match fs::read_dir(&dir) {
        Ok(read) => read,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for entry in read {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(item) = serde_json::from_slice::<FixRecord>(&bytes) {
                if item.project_id == project && valid_id(&item.id) {
                    out.push(item);
                }
            }
        }
    }
    out.sort_by(|a, b| {
        b.created_at_ms
            .cmp(&a.created_at_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(out)
}

pub fn load(data_dir: &Path, project: &str, id: &str) -> io::Result<FixRecord> {
    if !valid_id(project) || !valid_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid identifier",
        ));
    }
    let path = root(data_dir, project).join(format!("{id}.json"));
    let meta = fs::symlink_metadata(&path)?;
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "symlink rejected",
        ));
    }
    let item: FixRecord = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if item.id != id || item.project_id != project {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "id mismatch"));
    }
    Ok(item)
}

pub fn save(data_dir: &Path, fix: &FixRecord) -> io::Result<()> {
    if !valid_id(&fix.project_id) || !valid_id(&fix.id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid identifier",
        ));
    }
    let dir = root(data_dir, &fix.project_id);
    fs::create_dir_all(&dir)?;
    let target = dir.join(format!("{}.json", fix.id));
    let tmp = dir.join(format!(".{}.{}.tmp", fix.id, std::process::id()));
    let bytes = serde_json::to_vec_pretty(fix)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, &target)
}
