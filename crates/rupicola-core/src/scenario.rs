//! Versioned web capture scenarios and portable evidence manifests.

use crate::{capture, visual};
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    version: u32,
    name: String,
    #[serde(default)]
    project: Project,
    revision: Option<String>,
    url: String,
    timeout_ms: Option<u32>,
    profile: Profile,
    steps: Vec<Step>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    id: String,
    label: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    width: u32,
    height: u32,
    scale: f64,
    locale: String,
    color_scheme: ColorScheme,
    #[serde(default)]
    reduced_motion: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ColorScheme {
    Light,
    Dark,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Click { selector: String },
    Hover { selector: String },
    Fill { selector: String, value: String },
    Press { selector: String, key: String },
    Ready { selector: String },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Step {
    Click {
        selector: String,
    },
    Hover {
        selector: String,
    },
    Fill {
        selector: String,
        value: String,
    },
    Press {
        selector: String,
        key: String,
    },
    Ready {
        selector: String,
    },
    Checkpoint {
        id: String,
    },
    Clip {
        id: String,
        trigger: Action,
        samples: usize,
        interval_ms: u32,
    },
}

#[derive(Serialize)]
struct Environment {
    renderer: &'static str,
    platform: &'static str,
    #[serde(flatten)]
    profile: Profile,
}

#[derive(Serialize)]
pub struct Manifest {
    version: u32,
    scenario: String,
    project: Project,
    revision: Option<String>,
    created_at_ms: u64,
    environment: Environment,
    pub status: &'static str,
    inspection: &'static str,
    findings: Vec<Finding>,
    #[serde(flatten)]
    pub result: BrowserResult,
}

#[derive(Serialize)]
struct Finding {
    note: String,
    evidence: Option<PathBuf>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct BrowserResult {
    browser: Option<String>,
    pub blocker: Option<String>,
    checkpoints: Vec<Checkpoint>,
    clips: Vec<Clip>,
}

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    id: String,
    path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Clip {
    id: String,
    frames: Vec<Frame>,
    #[serde(default)]
    analysis_path: Option<PathBuf>,
    #[serde(default)]
    sheet_path: Option<PathBuf>,
    #[serde(default)]
    video_path: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Frame {
    path: PathBuf,
    target_ms: u64,
    elapsed_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    video_ms: Option<u64>,
}

#[derive(Serialize)]
struct ClipAnalysis {
    threshold: u8,
    frame_report: visual::Frames,
    samples: Vec<Sample>,
    selected_indices: Vec<usize>,
}

/// Longest identifier (project id, step id, app id label).
const MAX_IDENTIFIER_LEN: usize = 64;
/// Longest scenario step timeout, in milliseconds.
const MAX_TIMEOUT_MS: u32 = 30_000;
/// Largest viewport side, device scale range and total device pixels (8.3 MP,
/// about a 4K screen).
const MAX_VIEWPORT_SIDE: u32 = 4096;
const MIN_SCALE: f64 = 0.5;
const MAX_SCALE: f64 = 3.0;
const MAX_DEVICE_PIXELS: f64 = 8_300_000.0;
/// Most steps in one scenario.
const MAX_STEPS: usize = 64;
/// A clip: frame count and spacing ranges, and its longest span.
const CLIP_SAMPLES: std::ops::RangeInclusive<usize> = 2..=60;
const CLIP_INTERVAL_MS: std::ops::RangeInclusive<u32> = 1..=1000;
const MAX_CLIP_SPAN_MS: usize = 10_000;
/// Decoded RGBA bytes one clip may hold in memory (256 MiB).
const MAX_CLIP_DECODED_BYTES: f64 = 256.0 * 1024.0 * 1024.0;
/// Stalled and jumping frames kept in a clip's selection, at most.
const KEPT_STALLS: usize = 3;
const KEPT_JUMPS: usize = 3;
/// Frame-rate range for clip previews.
const MIN_PREVIEW_FPS: f64 = 1.0;
const MAX_PREVIEW_FPS: f64 = 120.0;
const MS_PER_SEC: f64 = 1000.0;
/// Contact sheet columns and thumbnail width.
const SHEET_COLUMNS: usize = 4;
const SHEET_THUMB_WIDTH: u32 = 240;

#[derive(Serialize)]
struct Sample {
    target_ms: u64,
    elapsed_ms: u64,
}

#[must_use]
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_LEN
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

impl Scenario {
    pub fn set_project(&mut self, id: String, label: String) {
        self.project = Project { id, label };
    }

    /// # Errors
    ///
    /// Returns an error if the scenario file cannot be read or decoded, or the scenario does not pass validation.
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
        let scenario: Self =
            toml::from_str(&content).map_err(|e| anyhow!("{}: {}", path.display(), e.message()))?;
        scenario.validate()?;
        Ok(scenario)
    }

    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("unsupported scenario version {}", self.version);
        }
        if self.name.trim().is_empty() {
            bail!("provide a scenario name");
        }
        if !["http://", "https://"]
            .iter()
            .any(|prefix| self.url.starts_with(prefix))
        {
            bail!("scenario URL must use HTTP or HTTPS");
        }
        if self
            .timeout_ms
            .is_some_and(|n| !(1..=MAX_TIMEOUT_MS).contains(&n))
        {
            bail!("timeout_ms must be in 1..={MAX_TIMEOUT_MS}");
        }
        let Profile {
            width,
            height,
            scale,
            ref locale,
            ..
        } = self.profile;
        if !(1..=MAX_VIEWPORT_SIDE).contains(&width)
            || !(1..=MAX_VIEWPORT_SIDE).contains(&height)
            || !(MIN_SCALE..=MAX_SCALE).contains(&scale)
            || locale.trim().is_empty()
            || (f64::from(width) * scale).ceil() * (f64::from(height) * scale).ceil()
                > MAX_DEVICE_PIXELS
        {
            bail!(
                "invalid viewport, device scale or locale (sides up to {MAX_VIEWPORT_SIDE}, scale \
                 {MIN_SCALE}–{MAX_SCALE}, at most {MAX_DEVICE_PIXELS} device pixels)"
            );
        }
        if self.steps.is_empty() || self.steps.len() > MAX_STEPS {
            bail!("provide between 1 and {MAX_STEPS} steps");
        }
        let mut ids = HashSet::new();
        let mut captures = 0;
        for step in &self.steps {
            let id = match step {
                Step::Checkpoint { id } => id,
                Step::Clip {
                    id,
                    samples,
                    interval_ms,
                    ..
                } => {
                    if !CLIP_SAMPLES.contains(samples)
                        || !CLIP_INTERVAL_MS.contains(interval_ms)
                        || (*samples - 1) * usize::try_from(*interval_ms).unwrap_or(usize::MAX)
                            > MAX_CLIP_SPAN_MS
                    {
                        bail!(
                            "clip {id} needs {}..={} samples spaced {}..={} ms across at most \
                             {MAX_CLIP_SPAN_MS} ms",
                            CLIP_SAMPLES.start(),
                            CLIP_SAMPLES.end(),
                            CLIP_INTERVAL_MS.start(),
                            CLIP_INTERVAL_MS.end()
                        );
                    }
                    let pixels =
                        (f64::from(width) * scale).ceil() * (f64::from(height) * scale).ceil();
                    let frame_count = u32::try_from(*samples + 1)?;
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "the channel count is tiny and exact in f64"
                    )]
                    let channels = visual::RGBA_CHANNELS as f64;
                    if pixels * f64::from(frame_count) * channels > MAX_CLIP_DECODED_BYTES {
                        bail!(
                            "clip {id} exceeds the {MAX_CLIP_DECODED_BYTES}-byte decoded frame budget"
                        );
                    }
                    id
                }
                _ => continue,
            };
            if !identifier(id) || !ids.insert(id.to_ascii_lowercase()) {
                bail!("invalid or duplicate evidence ID {id:?}");
            }
            captures += 1;
        }
        if captures == 0 {
            bail!("provide at least one checkpoint or clip");
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserRequest<'a> {
    scenario: &'a Scenario,
    output: &'a Path,
    module_path: Option<&'a Path>,
}

/// # Errors
///
/// Returns an error if capture inputs are invalid, the browser driver fails, or capture artifacts cannot be read or written.
pub fn web(
    scenario: Scenario,
    output: &Path,
    playwright_module: Option<&Path>,
    threshold: u8,
    video: bool,
) -> Result<Manifest> {
    scenario.validate()?;
    if !identifier(&scenario.project.id) || scenario.project.label.trim().is_empty() {
        bail!("provide a project label and simple project ID");
    }
    let output = new_run(output)?;
    let created_at_ms = now_ms()?;
    let mut result =
        drive_browser(&scenario, &output, playwright_module).unwrap_or_else(|blocker| {
            BrowserResult {
                blocker: Some(format!("{blocker:#}")),
                ..BrowserResult::default()
            }
        });
    let mut errors = result.blocker.take().into_iter().collect::<Vec<_>>();
    result
        .clips
        .retain_mut(|clip| match analyze_clip(&output, clip, threshold, video) {
            Ok(video_error) => {
                if let Some(error) = video_error {
                    errors.push(format!("clip {} preview: {error}", clip.id));
                }
                true
            }
            Err(error) => {
                errors.push(format!("clip {} analysis: {error:#}", clip.id));
                false
            }
        });
    result.blocker = (!errors.is_empty()).then(|| errors.join("; "));
    let samples = output.join("samples");
    if samples.exists() {
        fs::remove_dir_all(&samples).with_context(|| format!("{}", samples.display()))?;
    }
    let manifest = Manifest {
        version: 1,
        scenario: scenario.name,
        project: scenario.project,
        revision: scenario.revision,
        created_at_ms,
        environment: Environment {
            renderer: "chromium-headless",
            platform: std::env::consts::OS,
            profile: scenario.profile,
        },
        status: if result.blocker.is_some() {
            "blocked"
        } else {
            "complete"
        },
        inspection: "pending",
        findings: Vec::new(),
        result,
    };
    write_json(&output.join("manifest.json"), &manifest)?;
    Ok(manifest)
}

fn now_ms() -> Result<u64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(u64::try_from(elapsed.as_millis())?)
}

fn new_run(output: &Path) -> Result<PathBuf> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).with_context(|| format!("{}", parent.display()))?;
    fs::create_dir(output).with_context(|| format!("run directory {}", output.display()))?;
    Ok(fs::canonicalize(output)?)
}

fn drive_browser(
    scenario: &Scenario,
    output: &Path,
    module_path: Option<&Path>,
) -> Result<BrowserResult> {
    let module_path = module_path
        .map(fs::canonicalize)
        .transpose()
        .context("Playwright module path")?;
    fs::create_dir(output.join("checkpoints"))?;
    let request = serde_json::to_vec(&BrowserRequest {
        scenario,
        output,
        module_path: module_path.as_deref(),
    })?;
    let mut browser = Command::new("node")
        .args(["-e", include_str!("browser-capture.cjs")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("start Node.js")?;
    let input = browser
        .stdin
        .take()
        .expect("Node.js stdin is piped")
        .write_all(&request);
    let response = browser.wait_with_output().context("wait for Node.js")?;
    input.context("supply browser request")?;
    if !response.status.success() {
        bail!("browser driver exited with {}", response.status);
    }
    serde_json::from_slice(&response.stdout).context("read browser result")
}

#[expect(
    clippy::cast_precision_loss,
    reason = "video frame rates and offsets are estimates based on sampled timing"
)]
fn analyze_clip(
    output: &Path,
    clip: &mut Clip,
    threshold: u8,
    video: bool,
) -> Result<Option<String>> {
    let images = clip
        .frames
        .iter()
        .map(|frame| visual::load(&output.join(&frame.path)))
        .collect::<Result<Vec<_>, _>>()?;
    let report = visual::analyze_images(&images, threshold)?;
    let last = clip.frames.len() - 1;
    let mut selected = vec![0, last];
    selected.extend(report.first_change);
    selected.extend(report.settled_at);
    selected.push(last / 2);
    selected.extend(report.stalls.iter().take(KEPT_STALLS).copied());
    selected.extend(report.jumps.iter().take(KEPT_JUMPS).copied());
    selected.sort_unstable();
    selected.dedup();

    let directory = Path::new("clips").join(&clip.id);
    fs::create_dir_all(output.join(&directory))?;
    let mut video_error = None;
    let mut video_fps = None;
    if video {
        let video_path = directory.join("preview.mp4");
        let elapsed_ms = clip.frames[last].elapsed_ms.max(1);
        let options = capture::VideoOptions {
            fps: (last as f64 * MS_PER_SEC / elapsed_ms as f64)
                .clamp(MIN_PREVIEW_FPS, MAX_PREVIEW_FPS),
            ..capture::VideoOptions::default()
        };
        let path = output.join(&video_path);
        match capture::encode_rgba_images(&images, &path, &options) {
            Ok(_) => {
                video_fps = Some(options.fps);
                clip.video_path = Some(video_path);
            }
            Err(error) => {
                if path.is_file() {
                    fs::remove_file(&path).with_context(|| format!("{}", path.display()))?;
                }
                video_error = Some(format!("{error:#}"));
            }
        }
    }
    let analysis_path = directory.join("analysis.json");
    let analysis = ClipAnalysis {
        threshold,
        frame_report: report,
        samples: clip
            .frames
            .iter()
            .map(|frame| Sample {
                target_ms: frame.target_ms,
                elapsed_ms: frame.elapsed_ms,
            })
            .collect(),
        selected_indices: selected.clone(),
    };
    write_json(&output.join(&analysis_path), &analysis)?;
    let mut selected_frames = Vec::with_capacity(selected.len());
    for (index, mut frame) in clip.frames.drain(..).enumerate() {
        if selected.binary_search(&index).is_ok() {
            let target = directory.join(format!("{index:03}.png"));
            fs::rename(output.join(&frame.path), output.join(&target))?;
            frame.path = target;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "frame offsets are nonnegative and rounded to whole milliseconds at 1–120 FPS"
            )]
            let video_ms = video_fps.map(|fps| (index as f64 * MS_PER_SEC / fps).round() as u64);
            frame.video_ms = video_ms;
            selected_frames.push(frame);
        }
    }
    let sheet_path = directory.join("sheet.png");
    let files = selected_frames
        .iter()
        .map(|frame| output.join(&frame.path))
        .collect::<Vec<_>>();
    visual::save(
        &output.join(&sheet_path),
        &visual::sheet(&files, SHEET_COLUMNS, SHEET_THUMB_WIDTH)?,
    )?;
    clip.sheet_path = Some(sheet_path);
    clip.frames = selected_frames;
    clip.analysis_path = Some(analysis_path);
    Ok(video_error)
}

/// Add a human or agent observation without claiming visual verification from capture alone.
///
/// # Errors
///
/// Returns an error if the note is empty, the manifest is invalid or unreadable, the evidence is outside the run or missing, or the review cannot be written.
pub fn review(path: &Path, note: String, evidence: Option<PathBuf>) -> Result<()> {
    if note.trim().is_empty() {
        bail!("provide a concrete inspection note");
    }
    let path = fs::canonicalize(path).with_context(|| format!("{}", path.display()))?;
    let directory = path
        .parent()
        .ok_or_else(|| anyhow!("run directory is missing"))?;
    let lock = directory.join(".review.lock");
    fs::create_dir(&lock).with_context(|| format!("acquire {}", lock.display()))?;
    let _lock = ReviewLock(lock);
    let contents = fs::read(&path).with_context(|| format!("{}", path.display()))?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&contents).with_context(|| format!("{}", path.display()))?;
    if manifest["version"].as_u64() != Some(1) {
        bail!("capture review requires a v1 evidence manifest");
    }
    if let Some(reference) = &evidence {
        let artifacts: BrowserResult =
            serde_json::from_value(manifest.clone()).context("invalid evidence manifest")?;
        if reference
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
            || !artifacts.contains(reference)
            || !fs::canonicalize(directory.join(reference))
                .is_ok_and(|target| target.starts_with(directory) && target.is_file())
        {
            bail!("evidence must name an existing artifact relative to the run");
        }
    }
    let finding = serde_json::to_value(Finding { note, evidence })?;
    let object = manifest
        .as_object_mut()
        .ok_or_else(|| anyhow!("invalid evidence manifest"))?;
    object
        .entry("findings")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or_else(|| anyhow!("manifest findings must be an array"))?
        .push(finding);
    object.insert("inspection".into(), serde_json::json!("reviewed"));
    write_json(&path, &manifest)
}

impl BrowserResult {
    fn contains(&self, path: &Path) -> bool {
        self.checkpoints
            .iter()
            .any(|checkpoint| checkpoint.path == path)
            || self.clips.iter().any(|clip| {
                clip.frames.iter().any(|frame| frame.path == path)
                    || [&clip.analysis_path, &clip.sheet_path, &clip.video_path]
                        .iter()
                        .any(|artifact| artifact.as_deref() == Some(path))
            })
    }
}

struct ReviewLock(PathBuf);

impl Drop for ReviewLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)
        .and_then(|()| fs::rename(&tmp, path))
        .with_context(|| format!("{}", path.display()))
}
