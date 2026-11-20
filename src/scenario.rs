//! Versioned web capture scenarios and portable evidence manifests.

use crate::{capture, visual};
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
    #[serde(skip_deserializing)]
    System,
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

#[derive(Serialize)]
struct Sample {
    target_ms: u64,
    elapsed_ms: u64,
}

#[must_use]
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

impl Scenario {
    pub fn set_project(&mut self, id: String, label: String) {
        self.project = Project { id, label };
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let scenario: Self =
            toml::from_str(&content).map_err(|e| format!("{}: {}", path.display(), e.message()))?;
        scenario.validate()?;
        Ok(scenario)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("unsupported scenario version {}", self.version));
        }
        if self.name.trim().is_empty() {
            return Err("provide a scenario name".into());
        }
        if !["http://", "https://"]
            .iter()
            .any(|prefix| self.url.starts_with(prefix))
        {
            return Err("scenario URL must use HTTP or HTTPS".into());
        }
        if self.timeout_ms.is_some_and(|n| !(1..=30_000).contains(&n)) {
            return Err("timeout_ms must be in 1..=30000".into());
        }
        let Profile {
            width,
            height,
            scale,
            ref locale,
            ..
        } = self.profile;
        if !(1..=4096).contains(&width)
            || !(1..=4096).contains(&height)
            || !(0.5..=3.0).contains(&scale)
            || locale.trim().is_empty()
            || (f64::from(width) * scale).ceil() * (f64::from(height) * scale).ceil() > 8_300_000.0
        {
            return Err(
                "invalid viewport, device scale or locale (maximum 8.3 million device pixels)"
                    .into(),
            );
        }
        if self.steps.is_empty() || self.steps.len() > 64 {
            return Err("provide between 1 and 64 steps".into());
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
                    if !(2..=60).contains(samples)
                        || !(1..=1000).contains(interval_ms)
                        || (*samples - 1) as u32 * *interval_ms > 10_000
                    {
                        return Err(format!(
                            "clip {id} needs 2..=60 samples spaced 1..=1000 ms across at most 10 seconds"
                        ));
                    }
                    let pixels =
                        (f64::from(width) * scale).ceil() * (f64::from(height) * scale).ceil();
                    if pixels * (*samples + 1) as f64 * 4.0 > 268_435_456.0 {
                        return Err(format!("clip {id} exceeds 256 MiB decoded frame budget"));
                    }
                    id
                }
                _ => continue,
            };
            if !identifier(id) || !ids.insert(id.to_ascii_lowercase()) {
                return Err(format!("invalid or duplicate evidence ID {id:?}"));
            }
            captures += 1;
        }
        if captures == 0 {
            return Err("provide at least one checkpoint or clip".into());
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

pub fn web(
    scenario: Scenario,
    output: &Path,
    playwright_module: Option<&Path>,
    threshold: u8,
    video: bool,
) -> Result<Manifest, String> {
    scenario.validate()?;
    if !identifier(&scenario.project.id) || scenario.project.label.trim().is_empty() {
        return Err("provide a project label and simple project ID".into());
    }
    let output = new_run(output)?;
    let created_at_ms = now_ms()?;
    let mut result =
        drive_browser(&scenario, &output, playwright_module).unwrap_or_else(|blocker| {
            BrowserResult {
                blocker: Some(blocker),
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
                errors.push(format!("clip {} analysis: {error}", clip.id));
                false
            }
        });
    result.blocker = (!errors.is_empty()).then(|| errors.join("; "));
    let samples = output.join("samples");
    if samples.exists() {
        fs::remove_dir_all(&samples).map_err(|e| format!("{}: {e}", samples.display()))?;
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

/// Options for an isolated two-checkpoint capture of a COSMIC panel applet.
pub struct CosmicCapture {
    pub applet: PathBuf,
    pub desktop_entry: PathBuf,
    pub app_id: String,
    pub fixtures: PathBuf,
    pub symbolic_icon: Option<PathBuf>,
    pub x: u32,
    pub y: u32,
    pub project_id: String,
    pub project_label: String,
    pub revision: Option<String>,
}

pub fn cosmic(options: CosmicCapture, output: &Path) -> Result<Manifest, String> {
    if !identifier(&options.project_id) || options.project_label.trim().is_empty() {
        return Err("provide a project label and simple project ID".into());
    }
    if options.app_id.len() > 128 || !options.app_id.split('.').all(identifier) {
        return Err("provide a valid app ID (dot-separated labels)".into());
    }
    if options.x >= 1280 || options.y >= 800 {
        return Err("click coordinates must be inside the 1280x800 virtual display".into());
    }
    let output = new_run(output)?;
    let created_at_ms = now_ms()?;
    let result = drive_cosmic(&output, &options);
    let mut evidence = BrowserResult {
        blocker: result.err(),
        ..BrowserResult::default()
    };
    for id in ["closed", "open"] {
        let path = Path::new("checkpoints").join(format!("{id}.png"));
        if visual::load(&output.join(&path)).is_ok() {
            evidence.checkpoints.push(Checkpoint {
                id: id.into(),
                path,
            });
        }
    }
    if evidence.blocker.is_none() && evidence.checkpoints.len() != 2 {
        evidence.blocker = Some("one or more COSMIC checkpoints could not be read".into());
    }
    let manifest = Manifest {
        version: 1,
        scenario: "COSMIC panel applet click".into(),
        project: Project {
            id: options.project_id,
            label: options.project_label,
        },
        revision: options.revision,
        created_at_ms,
        environment: Environment {
            renderer: "cosmic-comp-winit-xvfb",
            platform: std::env::consts::OS,
            profile: Profile {
                width: 1280,
                height: 800,
                scale: 1.0,
                locale: "C".into(),
                color_scheme: ColorScheme::System,
                reduced_motion: false,
            },
        },
        status: if evidence.blocker.is_some() {
            "blocked"
        } else {
            "complete"
        },
        inspection: "pending",
        findings: Vec::new(),
        result: evidence,
    };
    write_json(&output.join("manifest.json"), &manifest)?;
    Ok(manifest)
}

fn drive_cosmic(output: &Path, options: &CosmicCapture) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("isolated COSMIC capture requires Linux".into());
    }
    let applet = fs::canonicalize(&options.applet).map_err(|e| format!("applet binary: {e}"))?;
    let desktop_entry =
        fs::canonicalize(&options.desktop_entry).map_err(|e| format!("desktop entry: {e}"))?;
    let fixtures = fs::canonicalize(&options.fixtures)
        .map_err(|e| format!("fixture executable directory: {e}"))?;
    if !fixtures.is_dir() {
        return Err("fixture executable path must be a directory".into());
    }
    let symbolic_icon = options
        .symbolic_icon
        .as_ref()
        .map(fs::canonicalize)
        .transpose()
        .map_err(|e| format!("symbolic SVG icon: {e}"))?
        .unwrap_or_default();
    let log = fs::File::create(output.join("native.log"))
        .map_err(|e| format!("native capture log: {e}"))?;
    let status = Command::new("/bin/bash")
        .args(["-c", include_str!("cosmic-capture.sh"), "cosmic-capture"])
        .arg(output)
        .arg(applet)
        .arg(desktop_entry)
        .arg(&options.app_id)
        .arg(fixtures)
        .arg(options.x.to_string())
        .arg(options.y.to_string())
        .arg(symbolic_icon)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .status()
        .map_err(|e| format!("start isolated COSMIC capture: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "isolated COSMIC capture exited with {status}; see native.log"
        ))
    }
}

fn now_ms() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    u64::try_from(elapsed.as_millis()).map_err(|e| e.to_string())
}

fn new_run(output: &Path) -> Result<PathBuf, String> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    fs::create_dir(output).map_err(|e| format!("run directory {}: {e}", output.display()))?;
    fs::canonicalize(output).map_err(|e| e.to_string())
}

fn drive_browser(
    scenario: &Scenario,
    output: &Path,
    module_path: Option<&Path>,
) -> Result<BrowserResult, String> {
    let module_path = module_path
        .map(fs::canonicalize)
        .transpose()
        .map_err(|e| format!("Playwright module path: {e}"))?;
    fs::create_dir(output.join("checkpoints")).map_err(|e| e.to_string())?;
    let request = serde_json::to_vec(&BrowserRequest {
        scenario,
        output,
        module_path: module_path.as_deref(),
    })
    .map_err(|e| e.to_string())?;
    let mut browser = Command::new("node")
        .args(["-e", include_str!("browser-capture.cjs")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("start Node.js: {e}"))?;
    let input = browser
        .stdin
        .take()
        .expect("Node.js stdin is piped")
        .write_all(&request);
    let response = browser
        .wait_with_output()
        .map_err(|e| format!("wait for Node.js: {e}"))?;
    input.map_err(|e| format!("supply browser request: {e}"))?;
    if !response.status.success() {
        return Err(format!("browser driver exited with {}", response.status));
    }
    serde_json::from_slice(&response.stdout).map_err(|e| format!("read browser result: {e}"))
}

fn analyze_clip(
    output: &Path,
    clip: &mut Clip,
    threshold: u8,
    video: bool,
) -> Result<Option<String>, String> {
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
    selected.extend(report.stalls.iter().take(3).copied());
    selected.extend(report.jumps.iter().take(3).copied());
    selected.sort_unstable();
    selected.dedup();

    let directory = Path::new("clips").join(&clip.id);
    fs::create_dir_all(output.join(&directory)).map_err(|e| e.to_string())?;
    let mut video_error = None;
    let mut video_fps = None;
    if video {
        let video_path = directory.join("preview.mp4");
        let elapsed_ms = clip.frames[last].elapsed_ms.max(1);
        let options = capture::VideoOptions {
            fps: (last as f64 * 1000.0 / elapsed_ms as f64).clamp(1.0, 120.0),
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
                    fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                }
                video_error = Some(error);
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
            fs::rename(output.join(&frame.path), output.join(&target))
                .map_err(|e| e.to_string())?;
            frame.path = target;
            frame.video_ms = video_fps.map(|fps| (index as f64 * 1000.0 / fps).round() as u64);
            selected_frames.push(frame);
        }
    }
    let sheet_path = directory.join("sheet.png");
    let files = selected_frames
        .iter()
        .map(|frame| output.join(&frame.path))
        .collect::<Vec<_>>();
    visual::save(&output.join(&sheet_path), &visual::sheet(&files, 4, 240)?)?;
    clip.sheet_path = Some(sheet_path);
    clip.frames = selected_frames;
    clip.analysis_path = Some(analysis_path);
    Ok(video_error)
}

/// Add a human or agent observation without claiming visual verification from capture alone.
pub fn review(path: &Path, note: String, evidence: Option<PathBuf>) -> Result<(), String> {
    if note.trim().is_empty() {
        return Err("provide a concrete inspection note".into());
    }
    let path = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let directory = path.parent().ok_or("run directory is missing")?;
    let lock = directory.join(".review.lock");
    fs::create_dir(&lock).map_err(|e| format!("acquire {}: {e}", lock.display()))?;
    let _lock = ReviewLock(lock);
    let contents = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&contents).map_err(|e| format!("{}: {e}", path.display()))?;
    if manifest["version"].as_u64() != Some(1) {
        return Err("capture review requires a v1 evidence manifest".into());
    }
    if let Some(reference) = &evidence {
        let artifacts: BrowserResult = serde_json::from_value(manifest.clone())
            .map_err(|e| format!("invalid evidence manifest: {e}"))?;
        if reference
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
            || !artifacts.contains(reference)
            || !fs::canonicalize(directory.join(reference))
                .is_ok_and(|target| target.starts_with(directory) && target.is_file())
        {
            return Err("evidence must name an existing artifact relative to the run".into());
        }
    }
    let finding = serde_json::to_value(Finding { note, evidence }).map_err(|e| e.to_string())?;
    let object = manifest
        .as_object_mut()
        .ok_or("invalid evidence manifest")?;
    object
        .entry("findings")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or("manifest findings must be an array")?
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

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)
        .and_then(|()| fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()))
}
