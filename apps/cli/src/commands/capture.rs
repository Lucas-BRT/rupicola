//! Capture commands: scenarios, COSMIC applets, review notes, probes and matrices.

use crate::{
    DEFAULT_MATRIX_MAX_RUNS, DEFAULT_MATRIX_TIMEOUT_SECS, DEFAULT_MAX_PAGES, EXIT_CLEAN,
    EXIT_ERROR, EXIT_FINDINGS, Result, anyhow, bail, block_on, json, print_capture, probe,
    publish::Destination, scenario, service, visual,
};
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct CaptureArgs {
    /// Versioned TOML scenario.
    scenario: PathBuf,
    /// New project run directory, typically .rupicola/captures/<run>.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Path to the installed Playwright Node.js module.
    #[arg(long)]
    playwright_module: Option<PathBuf>,
    /// Largest per-channel difference still treated as unchanged.
    #[arg(long, default_value_t = visual::DEFAULT_DIFF_THRESHOLD)]
    threshold: u8,
    /// Encode an approximate MP4 preview with ffmpeg.
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
        let manifest = scenario::web(
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

#[derive(clap::Args)]
pub struct CaptureCosmicArgs {
    #[arg(long)]
    applet: PathBuf,
    #[arg(long)]
    desktop_entry: PathBuf,
    #[arg(long)]
    app_id: String,
    /// Directory with fixture executables, such as a mock tailscale.
    #[arg(long)]
    fixtures: PathBuf,
    /// SVG installed as <app-id>-symbolic in the private XDG icon theme.
    #[arg(long)]
    symbolic_icon: Option<PathBuf>,
    #[arg(long)]
    project_id: Option<String>,
    #[arg(long)]
    project_label: Option<String>,
    #[arg(long)]
    revision: Option<String>,
    /// Click position in the private COSMIC display.
    #[arg(long, num_args = 2, value_names = ["X", "Y"], default_values_t = [scenario::DEFAULT_APPLET_CLICK_X, scenario::DEFAULT_APPLET_CLICK_Y])]
    click: Vec<u32>,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    destination: Destination,
}

impl CaptureCosmicArgs {
    pub(super) fn run(self) -> Result<u8> {
        let CaptureCosmicArgs {
            applet,
            desktop_entry,
            app_id,
            fixtures,
            symbolic_icon,
            project_id,
            project_label,
            revision,
            click,
            out,
            json: output_json,
            destination,
        } = self;
        let (out, project_id, project_label) = if let Some(out) = out {
            (
                Some(out),
                project_id.ok_or_else(|| anyhow!("--project-id is required with --out"))?,
                project_label.ok_or_else(|| anyhow!("--project-label is required with --out"))?,
            )
        } else {
            let (id, label) = service::detect_project()?;
            (None, id, label)
        };
        let run = destination.run_dir(out)?;
        let options = scenario::CosmicCapture {
            applet,
            desktop_entry,
            app_id,
            fixtures,
            symbolic_icon,
            project_id,
            project_label,
            revision,
            x: click[0],
            y: click[1],
        };
        let manifest = scenario::cosmic(options, run.dir())?;
        let code = print_capture(&manifest, run.dir(), output_json)?;
        block_on(destination.publish(run.dir(), output_json))??;
        Ok(code)
    }
}

#[derive(clap::Args)]
pub struct CaptureReviewArgs {
    /// Path to the run's manifest.json.
    manifest: PathBuf,
    /// Concrete observation from inspecting the available evidence.
    #[arg(long)]
    note: String,
    /// Optional evidence artifact path relative to the manifest.
    #[arg(long)]
    evidence: Option<PathBuf>,
}

impl CaptureReviewArgs {
    pub(super) fn run(self) -> Result<u8> {
        let CaptureReviewArgs {
            manifest,
            note,
            evidence,
        } = self;
        scenario::review(&manifest, note, evidence)?;
        println!("{}: inspection recorded", manifest.display());
        Ok(EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct ProbeArgs {
    /// Base HTTP(S) URL of the application under test.
    #[arg(long)]
    url: String,
    #[arg(long, value_enum, default_value = "responsive-matrix")]
    mode: probe::Mode,
    /// "all" (auto-crawl) or comma-separated routes (e.g. /,/projects,/settings).
    #[arg(long, default_value = "/")]
    pages: String,
    /// Comma-separated `WxH` or presets (mobile,tablet,laptop,desktop,wide).
    #[arg(long, default_value = "")]
    viewports: String,
    /// Comma-separated themes (dark,light).
    #[arg(long, default_value = "dark")]
    themes: String,
    #[arg(long, default_value_t = DEFAULT_MAX_PAGES)]
    max_pages: usize,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    playwright_module: Option<PathBuf>,
    #[arg(long)]
    video: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    destination: Destination,
}

impl ProbeArgs {
    pub(super) fn run(self) -> Result<u8> {
        let ProbeArgs {
            url,
            mode,
            pages,
            viewports,
            themes,
            max_pages,
            out,
            playwright_module,
            video,
            dry_run,
            json,
            destination,
        } = self;
        let run = destination.run_dir(out)?;
        let summary = probe::run(probe::Config {
            url,
            mode,
            pages,
            viewports,
            themes,
            max_pages,
            out: Some(run.dir().to_path_buf()),
            playwright_module,
            video,
            dry_run,
        })?;
        if !dry_run {
            block_on(destination.publish(&summary.output_dir, json))??;
        }
        if json {
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            println!(
                "Probe {} ({}) -> {} ({} pages, {} viewports, {} checkpoints, {} clips, {} findings)",
                summary.mode,
                summary.depth,
                summary.manifest_path.display(),
                summary.pages.len(),
                summary.viewports.len(),
                summary.checkpoints,
                summary.clips,
                summary.findings_count
            );
            for f in &summary.findings {
                println!("  - {}", f.note);
            }
        }
        Ok(if summary.blocker.is_some() {
            EXIT_ERROR
        } else if summary.findings_count > 0 {
            EXIT_FINDINGS
        } else {
            EXIT_CLEAN
        })
    }
}

#[derive(clap::Args)]
pub struct MatrixArgs {
    /// Shell command; it receives `RUPICOLA_OUT` (PNG path), `RUPICOLA_STATE`, `RUPICOLA_THEME`, `RUPICOLA_SCALE`,
    /// `RUPICOLA_TEXT_SCALE`, `RUPICOLA_SIZE`, `RUPICOLA_WIDTH`, `RUPICOLA_HEIGHT` and `RUPICOLA_LOCALE`.
    #[arg(long)]
    cmd: String,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, value_delimiter = ',', default_value = "default")]
    states: Vec<String>,
    #[arg(long, value_delimiter = ',', default_value = "light,dark")]
    themes: Vec<String>,
    #[arg(long, value_delimiter = ',', default_value = "1")]
    scales: Vec<String>,
    #[arg(long, value_delimiter = ',', default_value = "1")]
    text_scales: Vec<String>,
    #[arg(long, value_delimiter = ',', default_value = "800x600")]
    sizes: Vec<String>,
    #[arg(long, value_delimiter = ',', default_value = "en")]
    locales: Vec<String>,
    /// Refuse to run more combinations than this.
    #[arg(long, default_value_t = DEFAULT_MATRIX_MAX_RUNS)]
    max: usize,
    #[arg(long, default_value_t = DEFAULT_MATRIX_TIMEOUT_SECS)]
    timeout_secs: u64,
    /// Print the planned combinations without running anything.
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    json: bool,
}

impl MatrixArgs {
    pub(super) fn run(self) -> Result<u8> {
        let MatrixArgs {
            cmd,
            out,
            states,
            themes,
            scales,
            text_scales,
            sizes,
            locales,
            max,
            timeout_secs,
            dry_run,
            json: output_json,
        } = self;
        std::fs::create_dir_all(&out)?;
        let spec = visual::MatrixSpec {
            states,
            themes,
            scales,
            text_scales,
            sizes,
            locales,
        };
        let mut entries = visual::matrix_plan(&spec, &out);
        if entries.len() > max {
            bail!(
                "{} combinations exceed --max {max}; narrow the lists or raise the limit",
                entries.len()
            );
        }
        if !dry_run {
            visual::matrix_run(
                &mut entries,
                &cmd,
                std::time::Duration::from_secs(timeout_secs),
            );
            std::fs::write(
                out.join("manifest.json"),
                serde_json::to_string_pretty(&entries)?,
            )?;
        }
        if output_json {
            json(&entries)?;
        } else {
            for e in &entries {
                println!(
                    "{} {}",
                    if dry_run {
                        "plan"
                    } else if e.ok {
                        "ok  "
                    } else {
                        "FAIL"
                    },
                    e.name
                );
            }
        }
        Ok(u8::from(!dry_run && entries.iter().any(|e| !e.ok)))
    }
}
