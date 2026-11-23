//! Versioned web capture scenarios and portable evidence manifests.

use crate::visual;
use anyhow::{Context, Result, anyhow, bail};
use rupicola_capture::asynchronous::{CaptureBackend, run_plan};
use rupicola_capture::session;
use rupicola_protocol::UiAction;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
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

/// Folder of a run that holds the files a capture backend writes.
const CHECKPOINTS_DIR: &str = "checkpoints";
/// Folder of a run that holds the frames of a clip while it is analysed.
const SAMPLES_DIR: &str = "samples";
/// Prefix that makes a capture target a CSS selector rather than an element id.
const CSS_ENGINE: &str = "css=";
/// Separates the modifiers of a key chord, such as `Control+A`.
const CHORD_SEPARATOR: char = '+';
/// Width of the digits in a frame file name.
const FRAME_NAME_DIGITS: usize = 3;

impl From<&ColorScheme> for session::ColorScheme {
    fn from(scheme: &ColorScheme) -> Self {
        match scheme {
            ColorScheme::Light => Self::Light,
            ColorScheme::Dark => Self::Dark,
        }
    }
}

impl Action {
    fn target(selector: &str) -> String {
        format!("{CSS_ENGINE}{selector}")
    }

    /// What the capture backend does for this action.
    fn steps(&self) -> Vec<session::Step> {
        let act = |action| session::Step::Action { action };
        match self {
            Self::Click { selector } => vec![act(UiAction::Click {
                target_id: Self::target(selector),
            })],
            Self::Hover { selector } => vec![act(UiAction::Hover {
                target_id: Self::target(selector),
            })],
            Self::Fill { selector, value } => vec![act(UiAction::TypeText {
                target_id: Self::target(selector),
                text: value.clone(),
            })],
            Self::Press { selector, key } => {
                let mut parts = key
                    .split(CHORD_SEPARATOR)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                let key = parts.pop().unwrap_or_default();
                vec![
                    act(UiAction::Focus {
                        target_id: Self::target(selector),
                    }),
                    act(UiAction::PressKey {
                        key,
                        modifiers: parts,
                    }),
                ]
            }
            Self::Ready { selector } => vec![session::Step::Wait {
                condition: session::WaitCondition::Visible {
                    target_id: Self::target(selector),
                },
            }],
        }
    }
}

impl Step {
    /// The interval between the frames of a clip, for a clip.
    const fn clip_interval(&self) -> Option<u32> {
        match self {
            Self::Clip { interval_ms, .. } => Some(*interval_ms),
            _ => None,
        }
    }

    /// What the capture backend does for this step.
    fn steps(&self) -> Vec<session::Step> {
        match self {
            Self::Click { selector } => Action::Click {
                selector: selector.clone(),
            }
            .steps(),
            Self::Hover { selector } => Action::Hover {
                selector: selector.clone(),
            }
            .steps(),
            Self::Fill { selector, value } => Action::Fill {
                selector: selector.clone(),
                value: value.clone(),
            }
            .steps(),
            Self::Press { selector, key } => Action::Press {
                selector: selector.clone(),
                key: key.clone(),
            }
            .steps(),
            Self::Ready { selector } => Action::Ready {
                selector: selector.clone(),
            }
            .steps(),
            Self::Checkpoint { id } => vec![session::Step::Checkpoint { id: id.clone() }],
            Self::Clip {
                id,
                trigger,
                samples,
                interval_ms,
            } => {
                let span = u64::try_from(*samples).unwrap_or(u64::MAX) * u64::from(*interval_ms);
                let mut steps = vec![session::Step::StartRecording];
                steps.extend(trigger.steps());
                steps.push(session::Step::Wait {
                    condition: session::WaitCondition::Delay { ms: span },
                });
                steps.push(session::Step::StopRecording { id: id.clone() });
                steps
            }
        }
    }
}

impl Scenario {
    /// The capture plan this scenario asks a backend to run.
    #[must_use]
    pub fn plan(&self) -> session::Plan {
        let profile = &self.profile;
        let mut environment = session::Environment::new([profile.width, profile.height]);
        environment.scale = profile.scale;
        environment.color_scheme = Some((&profile.color_scheme).into());
        environment.reduced_motion = profile.reduced_motion;
        environment.warmup_ms = 0;
        environment.locale = Some(profile.locale.clone());
        environment.timeout_ms = self.timeout_ms.map(u64::from);
        // One recorder serves the whole plan, so it samples as fast as the fastest clip.
        environment.frame_interval_ms = self
            .steps
            .iter()
            .filter_map(Step::clip_interval)
            .min()
            .map(u64::from);
        session::Plan {
            target: session::Target {
                command: self.url.clone(),
                args: Vec::new(),
            },
            environment,
            steps: self.steps.iter().flat_map(Step::steps).collect(),
            allow_degraded: false,
        }
    }
}

/// Runs `scenario` on `backend` and writes the run, with its evidence manifest, under
/// `output`.
///
/// # Errors
///
/// Returns an error if capture inputs are invalid or artifacts cannot be read or written.
/// A backend that cannot run the scenario is not an error: it is the run's blocker.
pub async fn web(
    scenario: Scenario,
    output: &Path,
    backend: &dyn CaptureBackend,
    threshold: u8,
    video: bool,
) -> Result<Manifest> {
    let (output, created_at_ms) = begin(&scenario, output)?;
    let (result, errors) = capture(&scenario, &output, backend).await?;
    conclude(
        scenario,
        &output,
        created_at_ms,
        result,
        errors,
        threshold,
        video,
    )
}

/// Writes a run whose capture could not start, with `reason` as its blocker.
///
/// # Errors
///
/// Returns an error if the scenario is invalid or the run cannot be written.
pub fn blocked(scenario: Scenario, output: &Path, reason: String) -> Result<Manifest> {
    let (output, created_at_ms) = begin(&scenario, output)?;
    let result = BrowserResult::default();
    conclude(
        scenario,
        &output,
        created_at_ms,
        result,
        vec![reason],
        0,
        false,
    )
}

/// Validates the scenario and creates its run directory.
fn begin(scenario: &Scenario, output: &Path) -> Result<(PathBuf, u64)> {
    scenario.validate()?;
    if !identifier(&scenario.project.id) || scenario.project.label.trim().is_empty() {
        bail!("provide a project label and simple project ID");
    }
    let output = new_run(output)?;
    Ok((output, now_ms()?))
}

/// Runs the plan, then collects what the backend wrote, even when it stopped part way.
async fn capture(
    scenario: &Scenario,
    output: &Path,
    backend: &dyn CaptureBackend,
) -> Result<(BrowserResult, Vec<String>)> {
    let files = output.join(CHECKPOINTS_DIR);
    fs::create_dir(&files)?;
    let mut errors = Vec::new();
    if let Err(error) = run_plan(backend, &scenario.plan(), &files).await {
        errors.push(error.to_string());
    }
    let mut result = BrowserResult {
        browser: Some(backend.id().to_owned()),
        ..BrowserResult::default()
    };
    for step in &scenario.steps {
        match step {
            Step::Checkpoint { id } => {
                let path = Path::new(CHECKPOINTS_DIR).join(format!("{id}.png"));
                if output.join(&path).is_file() {
                    result.checkpoints.push(Checkpoint {
                        id: id.clone(),
                        path,
                    });
                }
            }
            Step::Clip {
                id,
                samples,
                interval_ms,
                ..
            } if files.join(format!("{id}.mp4")).is_file() => {
                match extract_frames(output, id, *samples, *interval_ms).await {
                    Ok(frames) => result.clips.push(Clip {
                        id: id.clone(),
                        frames,
                        analysis_path: None,
                        sheet_path: None,
                        video_path: None,
                    }),
                    Err(error) => errors.push(format!("clip {id} frames: {error:#}")),
                }
            }
            _ => {}
        }
    }
    Ok((result, errors))
}

/// Cuts a recorded clip into the frames its analysis looks at: one at the start and one
/// every `interval_ms` after it.
async fn extract_frames(
    output: &Path,
    id: &str,
    samples: usize,
    interval_ms: u32,
) -> Result<Vec<Frame>> {
    let directory = Path::new(SAMPLES_DIR).join(id);
    fs::create_dir_all(output.join(&directory))?;
    let recorded = output.join(CHECKPOINTS_DIR).join(format!("{id}.mp4"));
    let count = samples + 1;
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&recorded)
        .args(["-vf", &format!("fps=1000/{interval_ms}")])
        .args(["-frames:v", &count.to_string(), "-start_number", "0"])
        .arg(output.join(&directory).join("%03d.png"))
        .status()
        .await
        .context("run ffmpeg")?;
    if !status.success() {
        bail!("ffmpeg exited with {status}");
    }
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let path = directory.join(format!("{index:0FRAME_NAME_DIGITS$}.png"));
        if !output.join(&path).is_file() {
            break;
        }
        let elapsed_ms = u64::try_from(index)? * u64::from(interval_ms);
        frames.push(Frame {
            path,
            target_ms: elapsed_ms,
            elapsed_ms,
            video_ms: None,
        });
    }
    Ok(frames)
}

/// Analyses the clips, removes the scratch frames and writes the run's manifest.
fn conclude(
    scenario: Scenario,
    output: &Path,
    created_at_ms: u64,
    mut result: BrowserResult,
    mut errors: Vec<String>,
    threshold: u8,
    video: bool,
) -> Result<Manifest> {
    result
        .clips
        .retain_mut(|clip| match analyze_clip(output, clip, threshold, video) {
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
    let samples = output.join(SAMPLES_DIR);
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
    // The backend recorded the clip; it is the preview when asked for and scratch otherwise.
    let recorded = output
        .join(CHECKPOINTS_DIR)
        .join(format!("{}.mp4", clip.id));
    let mut video_error = None;
    if video {
        let preview = directory.join("preview.mp4");
        match fs::rename(&recorded, output.join(&preview)) {
            Ok(()) => clip.video_path = Some(preview),
            Err(error) => video_error = Some(error.to_string()),
        }
    } else if let Err(error) = fs::remove_file(&recorded) {
        video_error = Some(error.to_string());
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
            let video_ms = clip.video_path.is_some().then_some(frame.elapsed_ms);
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

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let data = serde_json::to_vec_pretty(value)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)
        .and_then(|()| fs::rename(&tmp, path))
        .with_context(|| format!("{}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENARIO: &str = r##"
version = 1
name = "Plan"
url = "http://127.0.0.1:1/"
timeout_ms = 8000
[project]
id = "demo"
label = "Demo"
[profile]
width = 640
height = 360
scale = 2.0
locale = "pt-BR"
color_scheme = "dark"
reduced_motion = true
[[steps]]
kind = "fill"
selector = "input#name"
value = "Ada"
[[steps]]
kind = "press"
selector = "input#name"
key = "Control+A"
[[steps]]
kind = "ready"
selector = "#box"
[[steps]]
kind = "clip"
id = "grow"
samples = 12
interval_ms = 100
[steps.trigger]
kind = "click"
selector = "button.go"
[[steps]]
kind = "clip"
id = "fast"
samples = 4
interval_ms = 40
[steps.trigger]
kind = "hover"
selector = "button.go"
[[steps]]
kind = "checkpoint"
id = "after"
"##;

    const DEMO_LOCALE: &str = "pt-BR";
    const DEMO_TIMEOUT_MS: u64 = 8000;
    const FASTEST_CLIP_MS: u64 = 40;
    const GROW_SPAN_MS: u64 = 1200;

    fn plan() -> session::Plan {
        toml::from_str::<Scenario>(SCENARIO).unwrap().plan()
    }

    fn target(selector: &str) -> String {
        format!("{CSS_ENGINE}{selector}")
    }

    #[test]
    fn the_environment_carries_the_profile_and_the_timing() {
        let environment = plan().environment;
        assert_eq!(environment.viewport, [640, 360]);
        assert!((environment.scale - 2.0).abs() < f64::EPSILON);
        assert_eq!(environment.color_scheme, Some(session::ColorScheme::Dark));
        assert!(environment.reduced_motion);
        assert_eq!(environment.locale.as_deref(), Some(DEMO_LOCALE));
        assert_eq!(environment.timeout_ms, Some(DEMO_TIMEOUT_MS));
        assert_eq!(environment.frame_interval_ms, Some(FASTEST_CLIP_MS));
    }

    #[test]
    fn interactions_become_actions_on_css_targets() {
        let steps = plan().steps;
        assert_eq!(
            steps[0],
            session::Step::Action {
                action: UiAction::TypeText {
                    target_id: target("input#name"),
                    text: "Ada".to_owned(),
                },
            }
        );
        // A key chord is focus on the control, then the key with its modifiers.
        assert_eq!(
            steps[1..3],
            [
                session::Step::Action {
                    action: UiAction::Focus {
                        target_id: target("input#name"),
                    },
                },
                session::Step::Action {
                    action: UiAction::PressKey {
                        key: "A".to_owned(),
                        modifiers: vec!["Control".to_owned()],
                    },
                },
            ]
        );
        assert_eq!(
            steps[3],
            session::Step::Wait {
                condition: session::WaitCondition::Visible {
                    target_id: target("#box"),
                },
            }
        );
    }

    #[test]
    fn a_clip_records_its_trigger_and_the_span_after_it() {
        let steps = plan().steps;
        assert_eq!(steps[4], session::Step::StartRecording);
        assert_eq!(
            steps[5],
            session::Step::Action {
                action: UiAction::Click {
                    target_id: target("button.go"),
                },
            }
        );
        assert_eq!(
            steps[6],
            session::Step::Wait {
                condition: session::WaitCondition::Delay { ms: GROW_SPAN_MS },
            }
        );
        assert_eq!(
            steps[7],
            session::Step::StopRecording {
                id: "grow".to_owned()
            }
        );
        assert_eq!(
            steps.last(),
            Some(&session::Step::Checkpoint {
                id: "after".to_owned()
            })
        );
    }
}
