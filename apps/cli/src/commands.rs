//! Subcommands: each takes its own argument struct and runs itself.

use crate::{
    AuthAction, Context, DEFAULT_MATRIX_MAX_RUNS, DEFAULT_MATRIX_TIMEOUT_SECS, DEFAULT_MAX_PAGES,
    DEFAULT_PALETTE_COLORS, DEFAULT_SHEET_COLUMNS, DEFAULT_SHEET_THUMB_WIDTH, EXIT_CLEAN,
    EXIT_ERROR, EXIT_FINDINGS, EXIT_UNSUPPORTED, FRAMEWORKS, PERCENT, Result, TOPICS, anyhow, bail,
    capture, check, design, discover, json, mcp, mcp_bridge, pipeline, print_capture, probe,
    propose, scan, scenario, service, visual,
};
use clap::Subcommand;
use std::{ffi::OsString, path::PathBuf};

#[derive(Subcommand)]
pub enum Command {
    /// Serve Rupicola tools over MCP stdio.
    Mcp,
    /// Bridge MCP stdio to a remote Rupicola panel using the stored bearer token.
    McpBridge(McpBridgeArgs),
    /// Manage credentials for remote Rupicola panels.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Identify declared UI framework dependencies without building or fetching.
    Discover(DiscoverArgs),
    /// Inspect Rust UI syntax. Findings are candidates for manual review.
    Scan(ScanArgs),
    /// Validate the skill catalog, direct links and source records.
    Check(CheckArgs),
    /// Compare two PNG screenshots (pixel difference, changed region, optional diff image).
    Diff(DiffArgs),
    /// Analyze a directory of animation frames (PNG) for stalls, jumps and settling.
    Frames(FramesArgs),
    /// Capture a declarative scenario in headless Chromium.
    Capture(CaptureArgs),
    /// Capture a COSMIC applet under an isolated nested compositor and panel (Linux).
    CaptureCosmic(CaptureCosmicArgs),
    /// Record a visual inspection note against a completed or blocked capture.
    CaptureReview(CaptureReviewArgs),
    /// Install and start the per-user evidence panel on Linux.
    Setup(SetupArgs),
    /// Link the current checkout to a project in the local panel.
    Link { project: String },
    /// Report this checkout's link and panel service status.
    Doctor,
    /// Open or record design context for the current project.
    Design {
        #[command(subcommand)]
        action: design::Action,
    },
    /// Manage detailed visual proposals, previews, and extractions.
    Propose {
        #[command(subcommand)]
        action: propose::Action,
    },
    /// Manage the project's visual problems triage queue.
    Problem {
        #[command(subcommand)]
        action: pipeline::ProblemAction,
    },
    /// Manage recorded UI fixes and before/after comparisons.
    Fix {
        #[command(subcommand)]
        action: pipeline::FixAction,
    },
    /// Inspect disk usage by project/date/fix, clean up unpinned captures, or offload to an external bucket.
    Storage {
        #[command(subcommand)]
        action: pipeline::StorageAction,
    },
    /// Run a combinatorial UI inspection matrix (pages × viewports × themes × interaction depth).
    Probe(ProbeArgs),
    /// Serve project evidence using the optional rupicola-panel executable.
    Serve(ServeArgs),
    /// Assemble PNG frames or a raw RGBA/BGRA stdin stream into a constant-framerate MP4 via ffmpeg.
    Video(VideoArgs),
    /// Extract a dominant-color palette and suggested roles from a PNG.
    Palette(PaletteArgs),
    /// Combine PNGs from a directory into one contact-sheet image.
    Sheet(SheetArgs),
    /// Run a capture command once per state, theme, scale, size and locale combination.
    Matrix(MatrixArgs),
    /// Check a JSON UI tree (rectangles, names, colors) for size, name, contrast and layout problems.
    Audit(AuditArgs),
    /// List the supported static rules.
    Rules(RulesArgs),
    /// List framework and UX module paths; does not load their contents.
    Modules(ModulesArgs),
}

#[derive(clap::Args)]
pub struct McpBridgeArgs {
    /// Base URL of the remote Rupicola panel or its /mcp endpoint.
    #[arg(long, default_value = "https://rupicola.lucasbrt.dev/mcp")]
    server_url: String,
    /// Optional project ID to scope the remote MCP session.
    #[arg(long)]
    project: Option<String>,
}

#[derive(clap::Args)]
pub struct DiscoverArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct ScanArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(FRAMEWORKS))]
    framework: Option<String>,
    #[arg(long)]
    include_tests: bool,
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct CheckArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct DiffArgs {
    before: PathBuf,
    after: PathBuf,
    /// Largest per-channel difference still treated as unchanged.
    #[arg(long, default_value_t = visual::DEFAULT_DIFF_THRESHOLD)]
    threshold: u8,
    /// Write a diff image (changed pixels in red).
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct FramesArgs {
    dir: PathBuf,
    #[arg(long, default_value_t = visual::DEFAULT_DIFF_THRESHOLD)]
    threshold: u8,
    #[arg(long)]
    json: bool,
}

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

#[derive(clap::Args)]
pub struct SetupArgs {
    /// Print the systemd unit without modifying the system.
    #[arg(long)]
    print_unit: bool,
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
}

#[derive(clap::Args)]
pub struct ServeArgs {
    /// Options forwarded to rupicola-panel, including --captures, --db and --port.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    options: Vec<OsString>,
}

#[derive(clap::Args)]
pub struct VideoArgs {
    /// Directory of PNG frames (omit when using --raw to read pixels from stdin).
    dir: Option<PathBuf>,
    /// Output MP4 file path.
    #[arg(long)]
    out: PathBuf,
    /// Target constant frame rate.
    #[arg(long, default_value_t = capture::DEFAULT_VIDEO_FPS)]
    fps: f64,
    /// Read raw pixel frames from stdin instead of a PNG directory.
    #[arg(long)]
    raw: bool,
    /// Frame width in pixels (required with --raw).
    #[arg(long)]
    width: Option<u32>,
    /// Frame height in pixels (required with --raw).
    #[arg(long)]
    height: Option<u32>,
    /// Pixel format for --raw input: rgba, premultiplied-rgba, bgra, premultiplied-bgra.
    #[arg(long, default_value = "rgba")]
    pixel_format: String,
    /// Also save lossless PNG frames to this directory when reading --raw stdin.
    #[arg(long)]
    save_frames: Option<PathBuf>,
    /// x264 CRF quality (lower is sharper; see `capture::MAX_CRF` for the top).
    #[arg(long, default_value_t = capture::DEFAULT_CRF)]
    crf: u8,
    /// x264 encoder preset (e.g. ultrafast, fast, medium).
    #[arg(long, default_value = "fast")]
    preset: String,
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct PaletteArgs {
    image: PathBuf,
    #[arg(long, default_value_t = DEFAULT_PALETTE_COLORS)]
    colors: usize,
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct SheetArgs {
    dir: PathBuf,
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = DEFAULT_SHEET_COLUMNS)]
    cols: usize,
    #[arg(long, default_value_t = DEFAULT_SHEET_THUMB_WIDTH)]
    thumb: u32,
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

#[derive(clap::Args)]
pub struct AuditArgs {
    tree: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct RulesArgs {
    #[arg(long)]
    json: bool,
}

#[derive(clap::Args)]
pub struct ModulesArgs {
    #[arg(long)]
    json: bool,
}

impl McpBridgeArgs {
    fn run(self) -> Result<u8> {
        let McpBridgeArgs {
            server_url,
            project,
        } = self;
        mcp_bridge::serve(&server_url, project.as_deref()).map(|()| EXIT_CLEAN)
    }
}

impl DiscoverArgs {
    fn run(self) -> Result<u8> {
        let DiscoverArgs {
            path,
            json: output_json,
        } = self;
        let result = discover::discover(&path)?;
        if output_json {
            json(&result)?;
        } else {
            for package in &result.packages {
                println!(
                    "{} [{}] {}",
                    package.name,
                    package
                        .frameworks
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    package.manifest.display()
                );
            }
            for d in &result.diagnostics {
                eprintln!("{}: {}", d.path.display(), d.message);
            }
            println!("{}", result.configuration);
        }
        Ok(u8::from(!result.diagnostics.is_empty()))
    }
}

impl ScanArgs {
    fn run(self) -> Result<u8> {
        let ScanArgs {
            path,
            framework,
            include_tests,
            json: output_json,
        } = self;
        let result = scan::scan(&path, framework.as_deref(), include_tests)?;
        if output_json {
            json(&result)?;
        } else {
            println!(
                "{} Rust files; {} parsed; {} unsupported; {} candidates",
                result.rust_files,
                result.parsed_files,
                result.unsupported_files.len(),
                result.findings.len()
            );
            for f in &result.findings {
                println!(
                    "{}:{}:{} [{}] {}",
                    f.file.display(),
                    f.line,
                    f.column,
                    f.rule,
                    f.message
                );
            }
            for d in &result.diagnostics {
                eprintln!("{}: {}", d.path.display(), d.message);
            }
            for limit in &result.limitations {
                println!("{limit}");
            }
        }
        Ok(if !result.diagnostics.is_empty() {
            EXIT_ERROR
        } else if !result.findings.is_empty() {
            EXIT_FINDINGS
        } else if result.parsed_files == 0 || !result.unsupported_files.is_empty() {
            EXIT_UNSUPPORTED
        } else {
            EXIT_CLEAN
        })
    }
}

impl CheckArgs {
    fn run(self) -> Result<u8> {
        let CheckArgs {
            path,
            json: output_json,
        } = self;
        let result = check::check(&path)?;
        if output_json {
            json(&result)?;
        } else {
            println!(
                "{} Markdown files checked; {} issues",
                result.markdown_files,
                result.diagnostics.len()
            );
            for d in &result.diagnostics {
                eprintln!("{}: {}", d.path.display(), d.message);
            }
        }
        Ok(u8::from(!result.diagnostics.is_empty()))
    }
}

impl DiffArgs {
    fn run(self) -> Result<u8> {
        let DiffArgs {
            before,
            after,
            threshold,
            out,
            json: output_json,
        } = self;
        let (a, b) = (visual::load(&before)?, visual::load(&after)?);
        let (report, image) = visual::diff_images(&a, &b, threshold);
        if let (Some(path), Some(image)) = (&out, &image) {
            visual::save(path, image)?;
        }
        if output_json {
            json(&report)?;
        } else if report.dimension_mismatch {
            println!(
                "different dimensions: {}x{} vs {}x{}",
                a.width, a.height, b.width, b.height
            );
        } else {
            println!(
                "{} of {} pixels changed ({:.2}%); mean delta {:.2}; max delta {}; region {:?}",
                report.changed_pixels,
                u64::from(report.width) * u64::from(report.height),
                report.changed_ratio * PERCENT,
                report.mean_abs_delta,
                report.max_delta,
                report.bbox
            );
        }
        Ok(if report.dimension_mismatch || report.changed_pixels > 0 {
            EXIT_FINDINGS
        } else {
            EXIT_CLEAN
        })
    }
}

impl FramesArgs {
    fn run(self) -> Result<u8> {
        let FramesArgs {
            dir,
            threshold,
            json: output_json,
        } = self;
        let report = visual::analyze_frames(&dir, threshold)?;
        if output_json {
            json(&report)?;
        } else {
            println!(
                "{} frames; changed from {:?} to {:?}; stalls {:?}; jumps {:?}; first-to-last {:.2}% changed",
                report.frame_count,
                report.first_change,
                report.settled_at,
                report.stalls,
                report.jumps,
                report.total_changed_ratio * PERCENT
            );
            for note in &report.notes {
                println!("note: {note}");
            }
        }
        Ok(
            if report.stalls.is_empty() && report.jumps.is_empty() && report.first_change.is_some()
            {
                EXIT_CLEAN
            } else {
                EXIT_FINDINGS
            },
        )
    }
}

impl CaptureArgs {
    fn run(self) -> Result<u8> {
        let CaptureArgs {
            scenario: path,
            out,
            playwright_module,
            threshold,
            video,
            json: output_json,
        } = self;
        let mut scenario = scenario::Scenario::load(&path)?;
        let out = if let Some(out) = out {
            out
        } else {
            let (id, label) = service::detect_project()?;
            scenario.set_project(id, label);
            service::output()?
        };
        let manifest = scenario::web(
            scenario,
            &out,
            playwright_module.as_deref(),
            threshold,
            video,
        )?;
        print_capture(&manifest, &out, output_json)
    }
}

impl CaptureCosmicArgs {
    fn run(self) -> Result<u8> {
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
        } = self;
        let (out, project_id, project_label) = if let Some(out) = out {
            (
                out,
                project_id.ok_or_else(|| anyhow!("--project-id is required with --out"))?,
                project_label.ok_or_else(|| anyhow!("--project-label is required with --out"))?,
            )
        } else {
            let (id, label) = service::detect_project()?;
            (service::output()?, id, label)
        };
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
        let manifest = scenario::cosmic(options, &out)?;
        print_capture(&manifest, &out, output_json)
    }
}

impl CaptureReviewArgs {
    fn run(self) -> Result<u8> {
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

impl SetupArgs {
    fn run(self) -> Result<u8> {
        let SetupArgs { print_unit } = self;
        service::setup(print_unit)?;
        Ok(EXIT_CLEAN)
    }
}

impl ProbeArgs {
    fn run(self) -> Result<u8> {
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
        } = self;
        let summary = probe::run(probe::Config {
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
        })?;
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

impl ServeArgs {
    fn run(self) -> Result<u8> {
        let ServeArgs { options } = self;
        let status = std::process::Command::new("rupicola-panel")
                .args(options)
                .status()
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        anyhow!("install the optional panel with cargo install --path apps/panel --locked and add it to PATH")
                    } else {
                        anyhow!(error).context("start rupicola-panel")
                    }
                })?;
        Ok(status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(1))
    }
}

impl VideoArgs {
    fn run(self) -> Result<u8> {
        let VideoArgs {
            dir,
            out,
            fps,
            raw,
            width,
            height,
            pixel_format,
            save_frames,
            crf,
            preset,
            json: output_json,
        } = self;
        let options = capture::VideoOptions {
            fps,
            crf,
            preset,
            ..capture::VideoOptions::default()
        };
        let report = if raw {
            if dir.is_some() {
                bail!("cannot combine a PNG directory with --raw");
            }
            let width = width.ok_or_else(|| anyhow!("--width is required with --raw"))?;
            let height = height.ok_or_else(|| anyhow!("--height is required with --raw"))?;
            let format = capture::PixelFormat::parse(&pixel_format)?;
            let stdin = std::io::stdin();
            capture::encode_raw_stream(
                stdin.lock(),
                width,
                height,
                format,
                &out,
                save_frames.as_deref(),
                &options,
            )?
        } else {
            let dir = dir.ok_or_else(|| {
                anyhow!("provide a PNG directory, or pass --raw to read from stdin")
            })?;
            if save_frames.is_some() {
                bail!("--save-frames is only valid with --raw");
            }
            capture::encode_png_dir(&dir, &out, &options)?
        };
        if output_json {
            json(&report)?;
        } else {
            println!(
                "{} ({} frames, {}x{} -> {}x{}, {:.2} fps, {:.2}s, {} bytes)",
                report.out.display(),
                report.frame_count,
                report.width,
                report.height,
                report.encoded_width,
                report.encoded_height,
                report.fps,
                report.duration_secs,
                report.bytes
            );
        }
        Ok(EXIT_CLEAN)
    }
}

impl PaletteArgs {
    fn run(self) -> Result<u8> {
        let PaletteArgs {
            image,
            colors,
            json: output_json,
        } = self;
        let report = visual::palette(&visual::load(&image)?, colors);
        if output_json {
            json(&report)?;
        } else {
            for c in &report.colors {
                println!("{} {:.1}%", c.hex, c.share * PERCENT);
            }
            println!("suggested: {:?}", report.suggested);
        }
        Ok(EXIT_CLEAN)
    }
}

impl SheetArgs {
    fn run(self) -> Result<u8> {
        let SheetArgs {
            dir,
            out,
            cols,
            thumb,
        } = self;
        let files = visual::list_pngs(&dir)?;
        visual::save(&out, &visual::sheet(&files, cols, thumb)?)?;
        println!("{} images, in order:", files.len());
        for f in &files {
            println!("{}", f.display());
        }
        Ok(EXIT_CLEAN)
    }
}

impl MatrixArgs {
    fn run(self) -> Result<u8> {
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

impl AuditArgs {
    fn run(self) -> Result<u8> {
        let AuditArgs {
            tree,
            json: output_json,
        } = self;
        let text = std::fs::read_to_string(&tree).with_context(|| format!("{}", tree.display()))?;
        let parsed: visual::Tree =
            serde_json::from_str(&text).with_context(|| format!("{}", tree.display()))?;
        let findings = visual::audit(&parsed);
        if output_json {
            json(&findings)?;
        } else {
            for f in &findings {
                println!("{} {}: {}", f.rule, f.node, f.message);
            }
            println!("{} nodes, {} findings", parsed.nodes.len(), findings.len());
        }
        Ok(if findings.is_empty() {
            EXIT_CLEAN
        } else {
            EXIT_FINDINGS
        })
    }
}

impl RulesArgs {
    fn run(self) -> Result<u8> {
        let RulesArgs { json: output_json } = self;
        if output_json {
            json(&scan::RULES)?;
        } else {
            for (id, description) in scan::RULES {
                println!("{id}: {description}");
            }
        }
        Ok(EXIT_CLEAN)
    }
}

impl ModulesArgs {
    fn run(self) -> Result<u8> {
        let ModulesArgs { json: output_json } = self;
        let modules: Vec<_> = FRAMEWORKS
            .iter()
            .map(|id| format!("reference/frameworks/{id}.md"))
            .chain(TOPICS.iter().map(|id| format!("reference/ux/{id}.md")))
            .collect();
        if output_json {
            json(&modules)?;
        } else {
            for module in modules {
                println!("{module}");
            }
        }
        Ok(EXIT_CLEAN)
    }
}

impl Command {
    pub fn run(self) -> Result<u8> {
        match self {
            Self::Mcp => Ok(mcp::serve().map(|()| EXIT_CLEAN)?),
            Self::McpBridge(args) => args.run(),
            Self::Auth {
                action: AuthAction::SetToken { token },
            } => mcp_bridge::set_token(token).map(|()| EXIT_CLEAN),
            Self::Discover(args) => args.run(),
            Self::Scan(args) => args.run(),
            Self::Check(args) => args.run(),
            Self::Diff(args) => args.run(),
            Self::Frames(args) => args.run(),
            Self::Capture(args) => args.run(),
            Self::CaptureCosmic(args) => args.run(),
            Self::CaptureReview(args) => args.run(),
            Self::Setup(args) => args.run(),
            Self::Link { project } => {
                service::link(project)?;
                Ok(EXIT_CLEAN)
            }
            Self::Doctor => {
                service::doctor()?;
                Ok(EXIT_CLEAN)
            }
            Self::Design { action } => {
                design::run(action)?;
                Ok(EXIT_CLEAN)
            }
            Self::Propose { action } => {
                propose::run(action)?;
                Ok(EXIT_CLEAN)
            }
            Self::Problem { action } => pipeline::run_problem(action),
            Self::Fix { action } => pipeline::run_fix(action),
            Self::Storage { action } => pipeline::run_storage(action),
            Self::Probe(args) => args.run(),
            Self::Serve(args) => args.run(),
            Self::Video(args) => args.run(),
            Self::Palette(args) => args.run(),
            Self::Sheet(args) => args.run(),
            Self::Matrix(args) => args.run(),
            Self::Audit(args) => args.run(),
            Self::Rules(args) => args.run(),
            Self::Modules(args) => args.run(),
        }
    }
}
