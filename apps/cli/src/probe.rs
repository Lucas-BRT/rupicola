use crate::{service, visual};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

/// Viewport presets (width, height): a small phone, a portrait tablet, a laptop,
/// a common desktop and a full-HD monitor.
const MOBILE_VIEWPORT: (u32, u32) = (320, 844);
const TABLET_VIEWPORT: (u32, u32) = (768, 1024);
const LAPTOP_VIEWPORT: (u32, u32) = (1280, 800);
const DESKTOP_VIEWPORT: (u32, u32) = (1440, 900);
const WIDE_VIEWPORT: (u32, u32) = (1920, 1080);
/// Accepted viewport sides, in CSS pixels.
const VIEWPORT_SIDE: std::ops::RangeInclusive<u32> = 240..=4096;
/// Most pages one probe visits, and how long a page may take to load.
const MAX_PAGES: usize = 50;
const PAGE_TIMEOUT_MS: u32 = 10_000;
/// UI audit findings reported per page and viewport.
const AUDIT_FINDINGS_PER_CELL: usize = 4;
/// Contact sheets: the matrix overview and each clip.
const MATRIX_SHEET_COLUMNS: usize = 3;
const MATRIX_SHEET_THUMB_WIDTH: u32 = 360;
const CLIP_SHEET_COLUMNS: usize = 5;
const CLIP_SHEET_THUMB_WIDTH: u32 = 320;
/// Share of changed pixels that makes a clip step count as motion.
const ACTIVE_STEP_RATIO: f64 = 0.005;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// 1 or N selected pages at 1 resolution (D1 snapshot & DOM/a11y audit).
    PageCheck,
    /// All auto-discovered pages at 1 resolution (D1 snapshot & DOM/a11y audit).
    AllPages,
    /// Selected or all pages across N viewports and themes (D2 responsive matrix + contact sheet).
    ResponsiveMatrix,
    /// Selected pages across N viewports with full interaction & motion map (D3).
    InteractionMap,
    /// All auto-discovered pages across N viewports/themes with full interaction & motion map (D3).
    Exhaustive,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::PageCheck => "page-check",
            Self::AllPages => "all-pages",
            Self::ResponsiveMatrix => "responsive-matrix",
            Self::InteractionMap => "interaction-map",
            Self::Exhaustive => "exhaustive",
        }
    }

    pub fn depth(self) -> &'static str {
        match self {
            Self::PageCheck | Self::AllPages => "d1",
            Self::ResponsiveMatrix => "d2",
            Self::InteractionMap | Self::Exhaustive => "d3",
        }
    }

    pub fn crawls_all(self, pages_arg: &str) -> bool {
        matches!(self, Self::AllPages | Self::Exhaustive)
            || pages_arg.trim().eq_ignore_ascii_case("all")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

pub struct Config {
    pub url: String,
    pub mode: Mode,
    pub pages: String,
    pub viewports: String,
    pub themes: String,
    pub max_pages: usize,
    pub out: Option<PathBuf>,
    pub playwright_module: Option<PathBuf>,
    pub video: bool,
    pub dry_run: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeRequest<'a> {
    origin: &'a str,
    pages_mode: &'static str,
    pages: Vec<String>,
    max_pages: usize,
    viewports: Vec<Viewport>,
    themes: Vec<String>,
    depth: &'static str,
    output: &'a Path,
    module_path: Option<&'a Path>,
    timeout_ms: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeResponse {
    browser: Option<String>,
    blocker: Option<String>,
    #[serde(default)]
    discovered_pages: Vec<String>,
    #[serde(default)]
    checkpoints: Vec<Checkpoint>,
    #[serde(default)]
    clips: Vec<Clip>,
    #[serde(default)]
    findings: Vec<Finding>,
    #[serde(default)]
    trees: Vec<CellTree>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub frames: Vec<Frame>,
    #[serde(default)]
    pub analysis_path: Option<PathBuf>,
    #[serde(default)]
    pub sheet_path: Option<PathBuf>,
    #[serde(default)]
    pub video_path: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Frame {
    pub path: PathBuf,
    pub target_ms: u64,
    pub elapsed_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Finding {
    pub note: String,
    pub evidence: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CellTree {
    route: String,
    viewport: String,
    theme: String,
    evidence: PathBuf,
    tree: serde_json::Value,
}

#[derive(Serialize)]
pub struct ProbeSummary {
    pub mode: String,
    pub depth: String,
    pub output_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub pages: Vec<String>,
    pub viewports: Vec<String>,
    pub themes: Vec<String>,
    pub total_cells: usize,
    pub checkpoints: usize,
    pub clips: usize,
    pub findings_count: usize,
    pub findings: Vec<Finding>,
    pub status: String,
    pub blocker: Option<String>,
}

pub fn parse_viewports(raw: &str, mode: Mode) -> Result<Vec<Viewport>> {
    let input = if raw.trim().is_empty() {
        match mode {
            Mode::PageCheck | Mode::AllPages => "1440x900",
            Mode::ResponsiveMatrix | Mode::InteractionMap | Mode::Exhaustive => {
                "320x844,768x1024,1440x900"
            }
        }
    } else {
        raw.trim()
    };
    let mut out = Vec::new();
    for item in input.split(',') {
        let token = item.trim();
        if token.is_empty() {
            continue;
        }
        let (w, h) = match token {
            "mobile" => MOBILE_VIEWPORT,
            "tablet" => TABLET_VIEWPORT,
            "laptop" => LAPTOP_VIEWPORT,
            "desktop" => DESKTOP_VIEWPORT,
            "wide" => WIDE_VIEWPORT,
            other => {
                let (ws, hs) = other
                    .split_once('x')
                    .or_else(|| other.split_once('X'))
                    .ok_or_else(|| anyhow!("invalid viewport {other:?} (expected WxH)"))?;
                let w: u32 = ws
                    .parse()
                    .with_context(|| format!("invalid width in {other:?}"))?;
                let h: u32 = hs
                    .parse()
                    .with_context(|| format!("invalid height in {other:?}"))?;
                (w, h)
            }
        };
        if !VIEWPORT_SIDE.contains(&w) || !VIEWPORT_SIDE.contains(&h) {
            bail!(
                "viewport {w}x{h} must be between {} and {} px",
                VIEWPORT_SIDE.start(),
                VIEWPORT_SIDE.end()
            );
        }
        out.push(Viewport {
            width: w,
            height: h,
        });
    }
    if out.is_empty() {
        bail!("provide at least one viewport");
    }
    Ok(out)
}

fn find_node() -> &'static str {
    if Path::new("/usr/lib/chatgpt/resources/cua_node/bin/node").is_file() {
        "/usr/lib/chatgpt/resources/cua_node/bin/node"
    } else {
        "node"
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the probe workflow keeps command setup, execution, and artifact processing in order"
)]
pub fn run(cfg: Config) -> Result<ProbeSummary> {
    if !cfg.url.starts_with("http://") && !cfg.url.starts_with("https://") {
        bail!("probe URL must start with http:// or https://");
    }
    let origin = cfg.url.trim_end_matches('/').to_owned();
    let viewports = parse_viewports(&cfg.viewports, cfg.mode)?;
    let themes: Vec<String> = cfg
        .themes
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| matches!(s.as_str(), "dark" | "light"))
        .collect();
    let themes = if themes.is_empty() {
        vec!["dark".into()]
    } else {
        themes
    };
    let crawl_all = cfg.mode.crawls_all(&cfg.pages);
    let seed_pages: Vec<String> = if crawl_all || cfg.pages.trim().is_empty() {
        vec!["/".into()]
    } else {
        cfg.pages
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                if s.starts_with('/') {
                    s.to_owned()
                } else {
                    format!("/{s}")
                }
            })
            .collect()
    };

    let output = match cfg.out {
        Some(dir) => dir,
        None => service::output()?,
    };
    let vp_labels: Vec<String> = viewports
        .iter()
        .map(|v| format!("{}x{}", v.width, v.height))
        .collect();

    if cfg.dry_run {
        let est_pages = if crawl_all {
            cfg.max_pages
        } else {
            seed_pages.len()
        };
        let total_cells = est_pages * viewports.len() * themes.len();
        return Ok(ProbeSummary {
            mode: cfg.mode.label().into(),
            depth: cfg.mode.depth().into(),
            output_dir: output.clone(),
            manifest_path: output.join("manifest.json"),
            pages: seed_pages,
            viewports: vp_labels,
            themes,
            total_cells,
            checkpoints: 0,
            clips: 0,
            findings_count: 0,
            findings: Vec::new(),
            status: "dry-run".into(),
            blocker: None,
        });
    }

    fs::create_dir_all(&output).with_context(|| format!("{}", output.display()))?;
    let script = output.join(".rupicola-browser-probe.cjs");
    fs::write(&script, include_str!("browser-probe.cjs"))
        .with_context(|| format!("{}", script.display()))?;

    let req = ProbeRequest {
        origin: &origin,
        pages_mode: if crawl_all { "all" } else { "selected" },
        pages: seed_pages,
        max_pages: cfg.max_pages.clamp(1, MAX_PAGES),
        viewports: viewports.clone(),
        themes: themes.clone(),
        depth: cfg.mode.depth(),
        output: &output,
        module_path: cfg.playwright_module.as_deref(),
        timeout_ms: PAGE_TIMEOUT_MS,
    };
    let payload = serde_json::to_vec(&req)?;

    let mut child = Command::new(find_node())
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to launch node for probe")?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&payload);
    }
    let out = child.wait_with_output()?;
    let _ = fs::remove_file(&script);

    let mut res: ProbeResponse = serde_json::from_slice(&out.stdout).unwrap_or(ProbeResponse {
        browser: None,
        blocker: Some(
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .next()
                .unwrap_or("probe script failed")
                .to_owned(),
        ),
        discovered_pages: Vec::new(),
        checkpoints: Vec::new(),
        clips: Vec::new(),
        findings: Vec::new(),
        trees: Vec::new(),
    });

    // Run visual::audit on every extracted DOM UI tree
    let trees_dir = output.join("trees");
    let _ = fs::create_dir_all(&trees_dir);
    for cell in &res.trees {
        let tree_path = trees_dir.join(format!(
            "{}-{}-{}.json",
            cell.route.trim_matches('/').replace('/', "_"),
            cell.viewport,
            cell.theme
        ));
        if let Ok(bytes) = serde_json::to_vec_pretty(&cell.tree) {
            let _ = fs::write(&tree_path, bytes);
        }
        if let Ok(tree) = serde_json::from_value::<visual::Tree>(cell.tree.clone()) {
            for issue in visual::audit(&tree)
                .into_iter()
                .take(AUDIT_FINDINGS_PER_CELL)
            {
                res.findings.push(Finding {
                    note: format!(
                        "measured · [{} @ {} {}] {} ({}): {}",
                        cell.route,
                        cell.viewport,
                        cell.theme,
                        issue.rule,
                        issue.node,
                        issue.message
                    ),
                    evidence: Some(cell.evidence.clone()),
                });
            }
        }
    }

    // Build matrix contact sheet when >1 checkpoints exist
    if res.checkpoints.len() > 1 {
        let shots_dir = output.join("checkpoints");
        let sheet_rel = PathBuf::from("checkpoints/matrix-sheet.png");
        if let Ok(files) = visual::list_pngs(&shots_dir)
            && let Ok(img) = visual::sheet(&files, MATRIX_SHEET_COLUMNS, MATRIX_SHEET_THUMB_WIDTH)
            && visual::save(&output.join(&sheet_rel), &img).is_ok()
        {
            res.checkpoints.insert(
                0,
                Checkpoint {
                    id: "matrix-contact-sheet".into(),
                    path: sheet_rel,
                },
            );
        }
    }

    // Analyze clips with visual::analyze_frames (+ optional MP4 video)
    for clip in &mut res.clips {
        let clip_dir = output.join("samples").join(&clip.id);
        if let Ok(report) = visual::analyze_frames(&clip_dir, visual::DEFAULT_DIFF_THRESHOLD) {
            let active_steps = report
                .step_changed_ratio
                .iter()
                .filter(|&&r| r >= ACTIVE_STEP_RATIO)
                .count();
            let is_instant_cut = active_steps <= 1 && report.stalls.is_empty();
            if !is_instant_cut && (!report.stalls.is_empty() || !report.jumps.is_empty()) {
                res.findings.push(Finding {
                    note: format!(
                        "measured · [{}] animation analysis: {} stalls, {} jumps",
                        clip.id,
                        report.stalls.len(),
                        report.jumps.len()
                    ),
                    evidence: None,
                });
            }
            let analysis_rel = PathBuf::from(format!("samples/{}/analysis.json", clip.id));
            if let Ok(json_bytes) = serde_json::to_vec_pretty(&serde_json::json!({
                "threshold": visual::DEFAULT_DIFF_THRESHOLD,
                "frame_report": report,
            })) && fs::write(output.join(&analysis_rel), json_bytes).is_ok()
            {
                clip.analysis_path = Some(analysis_rel);
            }
        }
        let sheet_rel = PathBuf::from(format!("samples/{}/sheet.png", clip.id));
        if let Ok(files) = visual::list_pngs(&clip_dir)
            && let Ok(img) = visual::sheet(&files, CLIP_SHEET_COLUMNS, CLIP_SHEET_THUMB_WIDTH)
            && visual::save(&output.join(&sheet_rel), &img).is_ok()
        {
            clip.sheet_path = Some(sheet_rel);
        }
        if cfg.video {
            let video_rel = PathBuf::from(format!("samples/{}/preview.mp4", clip.id));
            if rupicola_core::capture::encode_png_dir(
                &clip_dir,
                &output.join(&video_rel),
                &rupicola_core::capture::VideoOptions::default(),
            )
            .is_ok()
            {
                clip.video_path = Some(video_rel);
            }
        }
    }

    let (project_id, project_label) =
        service::detect_project().unwrap_or_else(|_| ("rupicola".into(), "rupicola".into()));
    let created_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let first_vp = viewports.first().cloned().unwrap_or(Viewport {
        width: DESKTOP_VIEWPORT.0,
        height: DESKTOP_VIEWPORT.1,
    });
    let status = if res.blocker.is_some() {
        "blocked"
    } else {
        "complete"
    };
    let inspection = if res.findings.is_empty() && res.blocker.is_none() {
        "reviewed"
    } else {
        "pending"
    };
    let scenario_label = format!(
        "Probe {} · {}p × {}vp × {}t",
        cfg.mode.label(),
        res.discovered_pages.len().max(1),
        viewports.len(),
        themes.len()
    );

    let manifest = serde_json::json!({
        "version": 1,
        "scenario": scenario_label,
        "project": { "id": project_id, "label": project_label },
        "revision": null,
        "created_at_ms": created_at_ms,
        "environment": {
            "renderer": "chromium-headless",
            "platform": std::env::consts::OS,
            "width": first_vp.width,
            "height": first_vp.height,
            "scale": 1.0,
            "locale": "en-US",
            "color_scheme": themes.first().map_or("dark", String::as_str),
            "reduced_motion": false
        },
        "status": status,
        "inspection": inspection,
        "findings": res.findings,
        "browser": res.browser,
        "blocker": res.blocker,
        "checkpoints": res.checkpoints,
        "clips": res.clips,
    });

    let manifest_path = output.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("{}", manifest_path.display()))?;

    let total_cells = res.discovered_pages.len() * viewports.len() * themes.len();
    Ok(ProbeSummary {
        mode: cfg.mode.label().into(),
        depth: cfg.mode.depth().into(),
        output_dir: output,
        manifest_path,
        pages: res.discovered_pages,
        viewports: vp_labels,
        themes,
        total_cells,
        checkpoints: res.checkpoints.len(),
        clips: res.clips.len(),
        findings_count: res.findings.len(),
        findings: res.findings,
        status: status.into(),
        blocker: res.blocker,
    })
}
