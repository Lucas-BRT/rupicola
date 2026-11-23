//! The capture backend a scenario runs on. A backend is a parameter of a capture, not a
//! command of its own: the scenario says what to capture, this says what captures it.

use crate::{block_on, scenario};
use anyhow::Result;
use rupicola_capture::asynchronous::{
    CaptureBackend, CdpBackend, CdpDiscovery as _, WebBackend, WebDiscovery as _,
};
use std::path::{Path, PathBuf};

/// Browser engine the Playwright backend launches.
const BROWSER_ENGINE: &str = "chromium";
/// Program that runs the Playwright driver.
const NODE: &str = "node";
const NO_PLAYWRIGHT: &str = "Playwright is unavailable. Install it, set RUPICOLA_PLAYWRIGHT_DIR or pass --playwright-module.";
const NO_CHROMIUM: &str = "no Chromium-family browser found in PATH";

/// Which backend captures a scenario.
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum Backend {
    /// A browser driven through Playwright: every action, clips, color scheme and scale.
    #[default]
    Playwright,
    /// A headless Chromium that only takes screenshots.
    Chromium,
}

impl Backend {
    /// The backend, ready to run; the error says why it cannot run here.
    async fn build(
        self,
        playwright_module: Option<&Path>,
    ) -> Result<Box<dyn CaptureBackend>, String> {
        match self {
            Self::Playwright => {
                let backend = if let Some(module) = playwright_module {
                    WebBackend {
                        browser_engine: BROWSER_ENGINE.to_owned(),
                        node_bin: PathBuf::from(NODE),
                        playwright_module: std::fs::canonicalize(module)
                            .map_err(|e| format!("Playwright module path: {e}"))?,
                    }
                } else {
                    WebBackend::discover(BROWSER_ENGINE)
                        .await
                        .ok_or_else(|| NO_PLAYWRIGHT.to_owned())?
                };
                Ok(Box::new(backend))
            }
            Self::Chromium => CdpBackend::discover()
                .await
                .map(|backend| Box::new(backend) as Box<dyn CaptureBackend>)
                .ok_or_else(|| NO_CHROMIUM.to_owned()),
        }
    }

    /// Captures `scenario` into the new run directory `out`. A backend that cannot run, or a
    /// plan it refuses, ends as the run's blocker.
    ///
    /// # Errors
    ///
    /// When the scenario is invalid or the run cannot be written.
    pub fn run(
        self,
        scenario: scenario::Scenario,
        out: &Path,
        playwright_module: Option<&Path>,
        threshold: u8,
        video: bool,
    ) -> Result<scenario::Manifest> {
        block_on(async {
            match self.build(playwright_module).await {
                Ok(backend) => scenario::web(scenario, out, &*backend, threshold, video).await,
                Err(reason) => scenario::blocked(scenario, out, reason),
            }
        })?
    }
}
