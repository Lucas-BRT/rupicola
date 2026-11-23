//! The capture command.

use crate::{
    Result, backend::Backend, block_on, print_capture, publish::Destination, scenario, service,
    visual,
};
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct CaptureArgs {
    /// Versioned TOML scenario.
    scenario: PathBuf,
    /// New project run directory, typically .rupicola/captures/<run>.
    #[arg(long)]
    out: Option<PathBuf>,
    /// What captures the scenario.
    #[arg(long, value_enum, default_value_t)]
    backend: Backend,
    /// Path to the installed Playwright Node.js module (the Playwright backend).
    #[arg(long)]
    playwright_module: Option<PathBuf>,
    /// Largest per-channel difference still treated as unchanged.
    #[arg(long, default_value_t = visual::DEFAULT_DIFF_THRESHOLD)]
    threshold: u8,
    /// Keep the recorded clips as MP4 previews.
    #[arg(long)]
    video: bool,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    destination: Destination,
}

impl CaptureArgs {
    pub(super) fn run(self) -> Result<u8> {
        let CaptureArgs {
            scenario: path,
            out,
            backend,
            playwright_module,
            threshold,
            video,
            json: output_json,
            destination,
        } = self;
        let mut scenario = scenario::Scenario::load(&path)?;
        if out.is_none() {
            let (id, label) = service::detect_project()?;
            scenario.set_project(id, label);
        }
        let run = destination.run_dir(out)?;
        let manifest = backend.run(
            scenario,
            run.dir(),
            playwright_module.as_deref(),
            threshold,
            video,
        )?;
        let code = print_capture(&manifest, run.dir(), output_json)?;
        block_on(destination.publish(run.dir(), output_json))??;
        Ok(code)
    }
}
