#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const DIRECTORY: &str = "problems";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemStatus {
    Open,
    InProgress,
    Intentional,
    FalsePositive,
    NeedsReprocess,
    Resolved,
}

impl ProblemStatus {
    pub const ALL: &'static [Self] = &[
        Self::Open,
        Self::InProgress,
        Self::Intentional,
        Self::FalsePositive,
        Self::NeedsReprocess,
        Self::Resolved,
    ];

    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in-progress",
            Self::Intentional => "intentional",
            Self::FalsePositive => "false-positive",
            Self::NeedsReprocess => "needs-reprocess",
            Self::Resolved => "resolved",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::InProgress => "In progress",
            Self::Intentional => "Intentional",
            Self::FalsePositive => "False positive",
            Self::NeedsReprocess => "Needs reprocess",
            Self::Resolved => "Resolved",
        }
    }

    #[must_use]
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "in-progress" => Some(Self::InProgress),
            "intentional" => Some(Self::Intentional),
            "false-positive" => Some(Self::FalsePositive),
            "needs-reprocess" => Some(Self::NeedsReprocess),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Critical,
    Major,
    Minor,
    Polish,
}

impl Severity {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Polish => "polish",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Polish => "Polish",
        }
    }

    #[must_use]
    pub fn from_slug(s: &str) -> Option<Self> {
        match s {
            "critical" => Some(Self::Critical),
            "major" => Some(Self::Major),
            "minor" => Some(Self::Minor),
            "polish" => Some(Self::Polish),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemOrigin {
    AgentMeasured,
    AgentVisual,
    UserReported,
}

impl ProblemOrigin {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AgentMeasured => "Agent · Measured",
            Self::AgentVisual => "Agent · Visual review",
            Self::UserReported => "User reported",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EvidenceAsset {
    pub run_key: String,
    pub asset_path: String,
    pub url: String,
    pub kind: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ViolatedRule {
    /// "global" or "project"
    pub scope: String,
    pub code: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProblemComment {
    pub author: String,
    pub text: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskStepLog {
    pub stage: String,
    pub progress_pct: u8,
    pub detail: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TaskExecution {
    #[serde(default)]
    pub agent_id: Option<String>,
    pub stage: String,
    pub progress_pct: u8,
    pub step_current: usize,
    pub step_total: usize,
    pub current_activity: String,
    #[serde(default)]
    pub logs: Vec<TaskStepLog>,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Problem {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: ProblemStatus,
    pub severity: Severity,
    pub origin: ProblemOrigin,
    pub route: String,
    pub viewport: String,
    pub theme: String,
    pub diagnosis: String,
    pub proposed_solution: String,
    #[serde(default)]
    pub violated_rules: Vec<ViolatedRule>,
    #[serde(default)]
    pub evidence_assets: Vec<EvidenceAsset>,
    #[serde(default)]
    pub comments: Vec<ProblemComment>,
    #[serde(default)]
    pub linked_fix_id: Option<String>,
    #[serde(default)]
    pub execution: Option<TaskExecution>,
    pub author: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[must_use]
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

#[must_use]
pub fn root(data_dir: &Path, project: &str) -> PathBuf {
    data_dir.join(DIRECTORY).join(project)
}

pub fn next_id() -> (String, u64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let ms = u64::try_from(now.as_millis()).unwrap_or(u64::MAX);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) % 1000;
    (format!("{}-{}{:03}", ms, std::process::id(), seq), ms)
}

/// # Errors
///
/// Returns an error if the project ID is invalid or the existing record directory cannot be opened or inspected.
pub fn entries(data_dir: &Path, project: &str) -> io::Result<Vec<Problem>> {
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
        if let Ok(bytes) = fs::read(&path)
            && let Ok(item) = serde_json::from_slice::<Problem>(&bytes)
            && item.project_id == project
            && valid_id(&item.id)
        {
            out.push(item);
        }
    }
    out.sort_by(|a, b| {
        b.created_at_ms
            .cmp(&a.created_at_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(out)
}

/// # Errors
///
/// Returns an error if an ID is invalid, the file cannot be read, the file is a symlink, the JSON is invalid, or the record IDs do not match.
pub fn load(data_dir: &Path, project: &str, id: &str) -> io::Result<Problem> {
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
    let item: Problem = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if item.id != id || item.project_id != project {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "id mismatch"));
    }
    Ok(item)
}

/// # Errors
///
/// Returns an error if an ID is invalid, serialization fails, or the record cannot be written.
pub fn save(data_dir: &Path, problem: &Problem) -> io::Result<()> {
    if !valid_id(&problem.project_id) || !valid_id(&problem.id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid identifier",
        ));
    }
    let dir = root(data_dir, &problem.project_id);
    fs::create_dir_all(&dir)?;
    let target = dir.join(format!("{}.json", problem.id));
    let tmp = dir.join(format!(".{}.{}.tmp", problem.id, std::process::id()));
    let bytes = serde_json::to_vec_pretty(problem)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, &target)
}

#[must_use]
pub fn is_suppressed(existing: &[Problem], route: &str, rule_code: &str, title: &str) -> bool {
    existing.iter().any(|p| {
        matches!(
            p.status,
            ProblemStatus::Intentional | ProblemStatus::FalsePositive
        ) && p.route == route
            && (p.title.eq_ignore_ascii_case(title)
                || p.violated_rules.iter().any(|r| r.code == rule_code))
    })
}

/// Infer severity, global rule code, rule title, and default solution from finding text.
#[must_use]
pub fn infer_rule(text: &str) -> (Severity, &'static str, &'static str, &'static str) {
    let lower = text.to_lowercase();
    if lower.contains("target-size") {
        (
            Severity::Major,
            "common.md#target-size-24px",
            "WCAG 2.5.8 / Rupicola Minimum Target Size (24×24px)",
            "Apply `min-block-size: 24px; min-inline-size: 24px; display: inline-flex; align-items: center;` on interactive controls and links.",
        )
    } else if lower.contains("horizontal overflow") || lower.contains("overflow") {
        (
            Severity::Critical,
            "layout.md#no-horizontal-overflow",
            "Zero Document Horizontal Overflow Across Breakpoints",
            "Constrain container width with `min-inline-size: 0`, `max-inline-size: 100%`, or responsive grid collapse at narrow viewports.",
        )
    } else if lower.contains("overlap") {
        (
            Severity::Critical,
            "layout.md#no-sibling-collision",
            "Spatial Separation & Non-Overlapping Sibling Controls",
            "Wrap flex/grid rows or enable scroll container clipping so interactive siblings never collide.",
        )
    } else if lower.contains("clipped") {
        (
            Severity::Minor,
            "typography.md#no-text-clipping",
            "Unclipped Labels & Readable Control Text",
            "Allow text wrapping or widen grid column track instead of truncating essential labels.",
        )
    } else if lower.contains("hover layout shift") {
        (
            Severity::Major,
            "motion.md#hover-stability",
            "Zero Layout Shift on Hover/Focus States",
            "Reserve border/outline space upfront (`border: 1px solid transparent`) and animate only color, opacity, or transform.",
        )
    } else if lower.contains("stalls") || lower.contains("jumps") || lower.contains("animation") {
        (
            Severity::Minor,
            "motion.md#choreographed-motion",
            "Smooth Transition Curve Without Stalls or Single-Frame Jumps",
            "Use `cubic-bezier(0.16, 1, 0.3, 1)` (150–220ms) for spatial motion or instant 0ms switches for tabs/dialogs.",
        )
    } else if lower.contains("broken images")
        || lower.contains("http 4")
        || lower.contains("http 5")
    {
        (
            Severity::Critical,
            "visual-verification.md#asset-integrity",
            "Zero Broken Media Assets or Failed Route Responses",
            "Verify asset paths, wait for lazy image decoding, and ensure all referenced routes/media return HTTP 200.",
        )
    } else if lower.contains("hairline") || lower.contains("seam") || lower.contains("gap") {
        (
            Severity::Minor,
            "common.md#hairline-grid",
            "Continuous 1px Hairline Structural Seams",
            "Use `gap: 1px; background: var(--line); border: 1px solid var(--line)` with `border-radius: 0` on inner cells.",
        )
    } else {
        (
            Severity::Minor,
            "visual-verification.md#visual-inspection",
            "Visual Craft & Structural Hierarchy Standard",
            "Inspect the referenced checkpoint asset and align spacing, contrast, and hierarchy with the active project Design direction.",
        )
    }
}

/// Return Rupicola UI/UX skill modules relevant to a problem's rules and diagnosis.
#[must_use]
pub fn recommended_skill_modules(problem: &Problem) -> Vec<&'static str> {
    let mut modules = vec!["reference/common.md", "reference/ux/content.md"];
    let mut push_unique = |path: &'static str| {
        if !modules.contains(&path) {
            modules.push(path);
        }
    };

    let mut combined = format!("{} {}", problem.title, problem.diagnosis).to_lowercase();
    for rule in &problem.violated_rules {
        combined.push(' ');
        combined.push_str(&rule.code.to_lowercase());
        combined.push(' ');
        combined.push_str(&rule.title.to_lowercase());
    }

    if [
        "layout", "overflow", "overlap", "grid", "spacing", "viewport", "seam", "hairline",
    ]
    .iter()
    .any(|kw| combined.contains(kw))
    {
        push_unique("reference/ux/layout.md");
        push_unique("reference/tactics/layout-structure.md");
    }
    if [
        "target-size",
        "contrast",
        "focus",
        "keyboard",
        "aria",
        "wcag",
        "a11y",
    ]
    .iter()
    .any(|kw| combined.contains(kw))
    {
        push_unique("reference/ux/accessibility.md");
        push_unique("reference/tactics/accessibility.md");
    }
    if [
        "motion",
        "hover",
        "animation",
        "stall",
        "jump",
        "transition",
    ]
    .iter()
    .any(|kw| combined.contains(kw))
    {
        push_unique("reference/ux/motion.md");
        push_unique("reference/tactics/states-feedback.md");
    }
    if ["typography", "clipped", "font", "truncat", "label"]
        .iter()
        .any(|kw| combined.contains(kw))
    {
        push_unique("reference/ux/typography.md");
        push_unique("reference/tactics/visual-system.md");
    }
    if ["color", "theme", "token", "palette", "contrast"]
        .iter()
        .any(|kw| combined.contains(kw))
    {
        push_unique("reference/ux/color.md");
        push_unique("reference/tactics/visual-system.md");
    }
    if [
        "visual-verification",
        "broken images",
        "asset",
        "http 4",
        "http 5",
    ]
    .iter()
    .any(|kw| combined.contains(kw))
    {
        push_unique("reference/visual-verification.md");
        push_unique("reference/critique.md");
    }
    push_unique("reference/tactics/index.md");
    modules
}

#[must_use]
pub fn infer_from_finding(
    project_id: &str,
    run_key: &str,
    note: &str,
    evidence: Option<&str>,
) -> Problem {
    let (id, now) = next_id();
    let origin = if note.starts_with("measured") || note.contains("Visto e medido") {
        ProblemOrigin::AgentMeasured
    } else {
        ProblemOrigin::AgentVisual
    };

    // Parse optional "[/route @ 1440×900 dark]" coordinate header
    let mut route = "/".to_string();
    let mut viewport = "1440×900".to_string();
    let mut theme = "dark".to_string();
    let mut body = note.trim().to_string();

    if let Some(start) = note.find('[')
        && let Some(end) = note[start..].find(']')
    {
        let inside = &note[start + 1..start + end];
        body = note[start + end + 1..]
            .trim_start_matches([':', ' ', '·'])
            .to_string();
        if let Some((r, rest)) = inside.split_once('@') {
            route = r.trim().to_string();
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if let Some(vp) = parts.first() {
                viewport = (*vp).to_string();
            }
            if let Some(th) = parts.get(1) {
                theme = (*th).to_string();
            }
        } else {
            route = inside.trim().to_string();
        }
    }

    let (severity, rule_code, rule_title, solution) = infer_rule(note);

    let title = if body.len() > 88 {
        format!("{}…", body.chars().take(85).collect::<String>())
    } else if body.is_empty() {
        "Visual inspection finding".to_string()
    } else {
        body
    };

    let mut evidence_assets = Vec::new();
    if let Some(ev) = evidence.filter(|s| !s.is_empty()) {
        let url = if ev.starts_with("/media/") {
            ev.to_string()
        } else {
            format!("/media/{run_key}/{ev}")
        };
        evidence_assets.push(EvidenceAsset {
            run_key: run_key.to_string(),
            asset_path: ev.to_string(),
            url,
            kind: "Screenshot".to_string(),
            label: format!("{route} · {viewport} · {theme}"),
        });
    }

    Problem {
        id,
        project_id: project_id.to_string(),
        title,
        status: ProblemStatus::Open,
        severity,
        origin,
        route,
        viewport,
        theme,
        diagnosis: note.to_string(),
        proposed_solution: solution.to_string(),
        violated_rules: vec![
            ViolatedRule {
                scope: "global".into(),
                code: rule_code.into(),
                title: rule_title.into(),
            },
            ViolatedRule {
                scope: "project".into(),
                code: "Design#flyio-copper-broadsheet".into(),
                title: "Fly.io Copper Broadsheet Direction".into(),
            },
        ],
        evidence_assets,
        comments: Vec::new(),
        linked_fix_id: None,
        execution: None,
        author: "agent".into(),
        created_at_ms: now,
        updated_at_ms: now,
    }
}
