//! Rupicola's envelope around a capture plan: who asked, for which project and scenario,
//! and which baseline run to mirror. The capture itself is `rupicola_capture::asynchronous`.

use rupicola_capture::asynchronous::{CaptureBackend, run_plan};
use rupicola_capture::session::{Bundle, CaptureError, Plan};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

const EMPTY_PROJECT_ID: &str = "CaptureEnvelope.project_id must not be empty";
const ZERO_VIEWPORT: &str = "CaptureEnvelope.plan.environment.viewport must be non-zero";
const NO_STEPS: &str = "CaptureEnvelope.plan.steps must contain at least one step";

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

/// Errors produced by [`CaptureEnvelope::validate`] and [`CaptureEnvelope::run`].
#[derive(Debug, thiserror::Error)]
pub enum CapturePipelineError {
    #[error("invalid capture envelope: {0}")]
    InvalidEnvelope(&'static str),
    #[error("capture failed: {0}")]
    Capture(#[from] CaptureError),
}

/// A capture plan plus the Rupicola context it runs in.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CaptureEnvelope {
    pub project_id: String,
    pub scenario: String,
    pub trigger: TriggerSource,
    pub plan: Plan,
    /// Optional baseline run ID so worktree/proposal captures mirror exact geometry.
    #[serde(default)]
    pub baseline_run_id: Option<String>,
}

/// What an envelope produced, and how long it took.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CaptureRun {
    pub envelope: CaptureEnvelope,
    pub bundle: Bundle,
    pub elapsed_ms: u64,
}

impl CaptureEnvelope {
    /// Validates structural invariants before anything starts.
    ///
    /// # Errors
    ///
    /// Returns [`CapturePipelineError::InvalidEnvelope`] if the envelope is malformed.
    pub fn validate(&self) -> Result<(), CapturePipelineError> {
        if self.project_id.trim().is_empty() {
            return Err(CapturePipelineError::InvalidEnvelope(EMPTY_PROJECT_ID));
        }
        if self.plan.environment.viewport.contains(&0) {
            return Err(CapturePipelineError::InvalidEnvelope(ZERO_VIEWPORT));
        }
        if self.plan.steps.is_empty() {
            return Err(CapturePipelineError::InvalidEnvelope(NO_STEPS));
        }
        Ok(())
    }

    /// Validates the envelope and runs its plan on `backend`, writing files under `out`.
    ///
    /// # Errors
    ///
    /// [`CapturePipelineError::InvalidEnvelope`] before anything starts, or the capture
    /// error from [`run_plan`].
    pub async fn run(
        &self,
        backend: &dyn CaptureBackend,
        out: &Path,
    ) -> Result<CaptureRun, CapturePipelineError> {
        self.validate()?;
        let started = Instant::now();
        let bundle = run_plan(backend, &self.plan, out).await?;
        Ok(CaptureRun {
            envelope: self.clone(),
            bundle,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rupicola_capture::asynchronous::CaptureSession;
    use rupicola_capture::session::{
        Capabilities, Checkpoint, Clip, Environment, Step, Target, WaitCondition,
    };
    use rupicola_protocol::UiAction;
    use std::path::PathBuf;

    const PROJECT: &str = "demo";
    const SCENARIO: &str = "proposal-preview";
    const BASELINE: &str = "0/baseline";
    const URL: &str = "http://127.0.0.1:4732";
    const VIEWPORT: [u32; 2] = [1440, 900];
    const CHECKPOINT: &str = "hero";
    const OUT: &str = "/tmp/out";
    const BACKEND_ID: &str = "mock";

    struct MockBackend;

    struct MockSession {
        out: PathBuf,
    }

    #[async_trait]
    impl CaptureBackend for MockBackend {
        fn id(&self) -> &'static str {
            BACKEND_ID
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }

        async fn start(
            &self,
            _target: &Target,
            _environment: &Environment,
            out: &Path,
        ) -> Result<Box<dyn CaptureSession>, CaptureError> {
            Ok(Box::new(MockSession {
                out: out.to_path_buf(),
            }))
        }
    }

    #[async_trait]
    impl CaptureSession for MockSession {
        async fn perform(&mut self, _action: &UiAction) -> Result<(), CaptureError> {
            Ok(())
        }

        async fn wait_for(&mut self, _condition: &WaitCondition) -> Result<(), CaptureError> {
            Ok(())
        }

        async fn screenshot(&mut self, id: &str) -> Result<Checkpoint, CaptureError> {
            Ok(Checkpoint {
                id: id.to_owned(),
                png: self.out.join(format!("{id}.png")),
                snapshot: None,
            })
        }

        async fn start_recording(&mut self) -> Result<(), CaptureError> {
            Ok(())
        }

        async fn stop_recording(&mut self, id: &str) -> Result<Clip, CaptureError> {
            Ok(Clip {
                id: id.to_owned(),
                mp4: self.out.join(format!("{id}.mp4")),
            })
        }

        async fn finish(self: Box<Self>) -> Result<(), CaptureError> {
            Ok(())
        }
    }

    fn envelope() -> CaptureEnvelope {
        CaptureEnvelope {
            project_id: PROJECT.to_owned(),
            scenario: SCENARIO.to_owned(),
            trigger: TriggerSource::McpAgent,
            plan: Plan {
                target: Target {
                    command: URL.to_owned(),
                    args: vec![],
                },
                environment: Environment::new(VIEWPORT),
                steps: vec![Step::Checkpoint {
                    id: CHECKPOINT.to_owned(),
                }],
                allow_degraded: false,
            },
            baseline_run_id: Some(BASELINE.to_owned()),
        }
    }

    #[tokio::test]
    async fn envelope_validates_and_runs_its_plan() {
        let run = envelope().run(&MockBackend, Path::new(OUT)).await.unwrap();
        assert_eq!(run.bundle.checkpoints.len(), 1);
        assert_eq!(run.bundle.checkpoints[0].id, CHECKPOINT);
        assert_eq!(run.envelope.baseline_run_id.as_deref(), Some(BASELINE));
    }

    #[tokio::test]
    async fn envelope_rejects_an_empty_project_zero_viewport_and_no_steps() {
        let mut no_project = envelope();
        no_project.project_id = "  ".to_owned();
        let mut flat = envelope();
        flat.plan.environment.viewport = [VIEWPORT[0], 0];
        let mut no_steps = envelope();
        no_steps.plan.steps.clear();
        for bad in [no_project, flat, no_steps] {
            assert!(matches!(
                bad.run(&MockBackend, Path::new(OUT)).await,
                Err(CapturePipelineError::InvalidEnvelope(_))
            ));
        }
    }

    #[tokio::test]
    async fn envelope_round_trips_through_json() {
        let original = envelope();
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(
            serde_json::from_str::<CaptureEnvelope>(&json).unwrap(),
            original
        );
    }
}
