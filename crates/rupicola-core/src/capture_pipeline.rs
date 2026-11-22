//! Layered capture architecture: Trigger -> `CapturePlan` -> Orchestrator
//! -> `CaptureBackend` -> Middleend -> Middleware -> Standardized `CaptureBundle`.

use rupicola_protocol::{ToolkitKind, UiAction, UiSnapshot, WidgetBox};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Origin that initiated a capture run.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerSource {
    ManualCli,
    ManualPanel,
    McpAgent,
    GitHook,
    Scheduler,
}

/// Target application under test or isolated proposal preview.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TargetKind {
    /// Live web server/URL or an isolated hidden proposal route with mock state.
    Web {
        base_url: String,
        #[serde(default)]
        isolated_preview_route: Option<String>,
    },
    /// Native desktop application launched in a headless/background environment.
    Desktop {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        protocol_socket: Option<PathBuf>,
        #[serde(default)]
        background: bool,
    },
}

/// Low-level capture backend responsible for frame/video/event extraction.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureBackendKind {
    Playwright,
    Cdp,
    CosmicHeadless,
    WaylandScreencopy,
    X11,
    Custom(String),
}

/// Middleend runtime environment (browser engine or display server + metrics).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MiddleendEnv {
    pub runtime: String,
    pub viewport: [u32; 2],
    #[serde(default = "default_scale")]
    pub scale_factor: f32,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub reduced_motion: bool,
}

const fn default_scale() -> f32 {
    1.0
}

fn default_theme() -> String {
    "light".to_owned()
}

/// Middleware adapter used to inspect widget trees and dispatch interactions.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MiddlewareKind {
    WebDom,
    RupicolaProtocol(ToolkitKind),
    AtSpi2,
    None,
}

/// Declarative step executed inside a [`CapturePlan`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "step", rename_all = "kebab-case")]
pub enum CaptureStep {
    Navigate {
        target: String,
    },
    Action {
        action: UiAction,
    },
    Snapshot {
        id: String,
        label: String,
    },
    RecordClip {
        id: String,
        duration_ms: u64,
        fps: u16,
    },
}

/// Unified, backend-agnostic capture plan submitted by any trigger.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapturePlan {
    pub project_id: String,
    pub scenario: String,
    pub trigger: TriggerSource,
    pub target: TargetKind,
    pub backend: CaptureBackendKind,
    pub middleend: MiddleendEnv,
    pub middleware: MiddlewareKind,
    #[serde(default)]
    pub steps: Vec<CaptureStep>,
    /// Optional baseline run ID so worktree/proposal captures mirror exact geometry.
    #[serde(default)]
    pub baseline_run_id: Option<String>,
}

/// Structured errors produced by [`CapturePlan::validate`], [`CaptureBackend::execute`],
/// and [`Orchestrator::run`].
#[derive(Debug, thiserror::Error)]
pub enum CapturePipelineError {
    #[error("invalid capture plan: {0}")]
    InvalidPlan(&'static str),
    #[error("no registered capture backend for {0:?} with target")]
    NoBackend(CaptureBackendKind),
    #[error("capture execution failed: {0}")]
    Execution(#[from] anyhow::Error),
}

impl CapturePlan {
    /// Validates structural invariants before dispatching to a backend.
    ///
    /// # Errors
    ///
    /// Returns [`CapturePipelineError::InvalidPlan`] if the plan is malformed or mismatched.
    pub fn validate(&self) -> Result<(), CapturePipelineError> {
        if self.project_id.trim().is_empty() {
            return Err(CapturePipelineError::InvalidPlan(
                "CapturePlan.project_id must not be empty",
            ));
        }
        if self.middleend.viewport[0] == 0 || self.middleend.viewport[1] == 0 {
            return Err(CapturePipelineError::InvalidPlan(
                "CapturePlan.middleend.viewport must be non-zero",
            ));
        }
        if self.steps.is_empty() {
            return Err(CapturePipelineError::InvalidPlan(
                "CapturePlan.steps must contain at least one step",
            ));
        }
        match (&self.target, &self.backend) {
            (
                TargetKind::Web { base_url, .. },
                CaptureBackendKind::Playwright
                | CaptureBackendKind::Cdp
                | CaptureBackendKind::Custom(_),
            ) => {
                if base_url.trim().is_empty() {
                    return Err(CapturePipelineError::InvalidPlan(
                        "Web target requires a non-empty base_url",
                    ));
                }
            }
            (TargetKind::Web { .. }, _) => {
                return Err(CapturePipelineError::InvalidPlan(
                    "Web target requires Playwright, Cdp, or Custom backend",
                ));
            }
            (
                TargetKind::Desktop { command, .. },
                CaptureBackendKind::CosmicHeadless
                | CaptureBackendKind::WaylandScreencopy
                | CaptureBackendKind::X11
                | CaptureBackendKind::Custom(_),
            ) => {
                if command.trim().is_empty() {
                    return Err(CapturePipelineError::InvalidPlan(
                        "Desktop target requires a non-empty command",
                    ));
                }
            }
            (TargetKind::Desktop { .. }, _) => {
                return Err(CapturePipelineError::InvalidPlan(
                    "Desktop target requires CosmicHeadless, WaylandScreencopy, X11, or Custom backend",
                ));
            }
        }
        Ok(())
    }
}

/// Standardized media item produced by any backend.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapturedCheckpoint {
    pub id: String,
    pub label: String,
    pub image_path: PathBuf,
    pub dimensions: [u32; 2],
    #[serde(default)]
    pub boxes: Vec<WidgetBoxJson>,
}

/// Serializable widget bounding box attached to a checkpoint or clip.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WidgetBoxJson {
    pub id: String,
    pub role: String,
    #[serde(default)]
    pub label: Option<String>,
    pub pixel_box: [u32; 4],
}

impl WidgetBoxJson {
    #[must_use]
    pub fn from_widget_box(w: &WidgetBox, scale_factor: f32) -> Self {
        Self {
            id: w.id.clone(),
            role: w.role.clone(),
            label: w.label.clone(),
            pixel_box: w.bounds.to_pixel_box(scale_factor),
        }
    }
}

/// Standardized video clip produced by any backend.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapturedClip {
    pub id: String,
    pub video_path: PathBuf,
    pub duration_ms: u64,
    pub dimensions: [u32; 2],
}

/// Backend-agnostic bundle returned by [`Orchestrator::run`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CaptureBundle {
    pub plan: CapturePlan,
    pub checkpoints: Vec<CapturedCheckpoint>,
    pub clips: Vec<CapturedClip>,
    #[serde(default)]
    pub snapshots: Vec<UiSnapshot>,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Trait implemented by each platform/capture backend.
pub trait CaptureBackend {
    fn kind(&self) -> CaptureBackendKind;
    fn supports(&self, target: &TargetKind) -> bool;

    /// Executes `plan` and writes artifacts under `output_dir`.
    ///
    /// # Errors
    ///
    /// Returns [`CapturePipelineError`] if the environment cannot start or capture fails.
    fn execute(
        &self,
        plan: &CapturePlan,
        output_dir: &Path,
    ) -> Result<CaptureBundle, CapturePipelineError>;
}

/// Single entry point that validates a [`CapturePlan`] and dispatches it
/// to a compatible [`CaptureBackend`].
#[derive(Default)]
pub struct Orchestrator {
    backends: Vec<Box<dyn CaptureBackend>>,
}

impl Orchestrator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    pub fn register(&mut self, backend: Box<dyn CaptureBackend>) {
        self.backends.push(backend);
    }

    /// Validates `plan` and executes it on the matching registered backend.
    ///
    /// # Errors
    ///
    /// Returns [`CapturePipelineError`] if validation fails, no matching backend is registered,
    /// or execution fails.
    pub fn run(
        &self,
        plan: &CapturePlan,
        output_dir: &Path,
    ) -> Result<CaptureBundle, CapturePipelineError> {
        plan.validate()?;
        let backend = self
            .backends
            .iter()
            .find(|b| b.kind() == plan.backend && b.supports(&plan.target))
            .ok_or_else(|| CapturePipelineError::NoBackend(plan.backend.clone()))?;
        backend.execute(plan, output_dir)
    }
}
