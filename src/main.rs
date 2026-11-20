mod design;
mod mcp;
mod mcp_bridge;
mod pipeline;
mod probe;
mod propose;
mod service;
use clap::{Parser, Subcommand};
use rupicola::{
    FRAMEWORKS, TOPICS, capture, check, design_record, discover, fix_record, problem_record,
    proposal_record, scan, scenario, storage, visual,
};
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    version,
    about = "Rust-native tools for AI-assisted interface development"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Serve Rupicola tools over MCP stdio.
    Mcp,
    /// Bridge MCP stdio to a remote Rupicola panel using the stored bearer token.
    McpBridge {
        /// Base URL of the remote Rupicola panel or its /mcp endpoint.
        #[arg(long, default_value = "https://rupicola.lucasbrt.dev/mcp")]
        server_url: String,
        /// Optional project ID to scope the remote MCP session.
        #[arg(long)]
        project: Option<String>,
    },
    /// Manage credentials for remote Rupicola panels.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Identify declared UI framework dependencies without building or fetching.
    Discover {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Inspect Rust UI syntax. Findings are candidates for manual review.
    Scan {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(FRAMEWORKS))]
        framework: Option<String>,
        #[arg(long)]
        include_tests: bool,
        #[arg(long)]
        json: bool,
    },
    /// Validate the skill catalog, direct links and source records.
    Check {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Compare two PNG screenshots (pixel difference, changed region, optional diff image).
    Diff {
        before: PathBuf,
        after: PathBuf,
        /// Largest per-channel difference still treated as unchanged.
        #[arg(long, default_value_t = 8)]
        threshold: u8,
        /// Write a diff image (changed pixels in red).
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Analyze a directory of animation frames (PNG) for stalls, jumps and settling.
    Frames {
        dir: PathBuf,
        #[arg(long, default_value_t = 8)]
        threshold: u8,
        #[arg(long)]
        json: bool,
    },
    /// Capture a declarative scenario in headless Chromium.
    Capture {
        /// Versioned TOML scenario.
        scenario: PathBuf,
        /// New project run directory, typically .rupicola/captures/<run>.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Path to the installed Playwright Node.js module.
        #[arg(long)]
        playwright_module: Option<PathBuf>,
        /// Largest per-channel difference still treated as unchanged.
        #[arg(long, default_value_t = 8)]
        threshold: u8,
        /// Encode an approximate MP4 preview with ffmpeg.
        #[arg(long)]
        video: bool,
        #[arg(long)]
        json: bool,
    },
    /// Capture a COSMIC applet under an isolated nested compositor and panel (Linux).
    CaptureCosmic {
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
        /// Click position in the private 1280x800 display.
        #[arg(long, num_args = 2, value_names = ["X", "Y"], default_values = ["640", "18"])]
        click: Vec<u32>,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Record a visual inspection note against a completed or blocked capture.
    CaptureReview {
        /// Path to the run's manifest.json.
        manifest: PathBuf,
        /// Concrete observation from inspecting the available evidence.
        #[arg(long)]
        note: String,
        /// Optional evidence artifact path relative to the manifest.
        #[arg(long)]
        evidence: Option<PathBuf>,
    },
    /// Install and start the per-user evidence panel on Linux.
    Setup {
        /// Print the systemd unit without modifying the system.
        #[arg(long)]
        print_unit: bool,
    },
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
    Probe {
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
        #[arg(long, default_value_t = 20)]
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
    },
    /// Serve project evidence using the optional rupicola-panel executable.
    Serve {
        /// Options forwarded to rupicola-panel, including --captures, --db and --port.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        options: Vec<OsString>,
    },
    /// Assemble PNG frames or a raw RGBA/BGRA stdin stream into a constant-framerate MP4 via ffmpeg.
    Video {
        /// Directory of PNG frames (omit when using --raw to read pixels from stdin).
        dir: Option<PathBuf>,
        /// Output MP4 file path.
        #[arg(long)]
        out: PathBuf,
        /// Target constant frame rate.
        #[arg(long, default_value_t = 60.0)]
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
        /// x264 CRF quality (0..=51, lower is sharper).
        #[arg(long, default_value_t = 18)]
        crf: u8,
        /// x264 encoder preset (e.g. ultrafast, fast, medium).
        #[arg(long, default_value = "fast")]
        preset: String,
        #[arg(long)]
        json: bool,
    },
    /// Extract a dominant-color palette and suggested roles from a PNG.
    Palette {
        image: PathBuf,
        #[arg(long, default_value_t = 8)]
        colors: usize,
        #[arg(long)]
        json: bool,
    },
    /// Combine PNGs from a directory into one contact-sheet image.
    Sheet {
        dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 4)]
        cols: usize,
        #[arg(long, default_value_t = 320)]
        thumb: u32,
    },
    /// Run a capture command once per state, theme, scale, size and locale combination.
    Matrix {
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
        #[arg(long, default_value_t = 48)]
        max: usize,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
        /// Print the planned combinations without running anything.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Check a JSON UI tree (rectangles, names, colors) for size, name, contrast and layout problems.
    Audit {
        tree: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// List the supported static rules.
    Rules {
        #[arg(long)]
        json: bool,
    },
    /// List framework and UX module paths; does not load their contents.
    Modules {
        #[arg(long)]
        json: bool,
    },
}
fn json(value: &impl serde::Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}
fn print_capture(
    manifest: &scenario::Manifest,
    out: &std::path::Path,
    output_json: bool,
) -> Result<u8, String> {
    if output_json {
        json(manifest)?;
    } else {
        println!(
            "{}: {}",
            out.join("manifest.json").display(),
            manifest.status
        );
        if let Some(blocker) = &manifest.result.blocker {
            eprintln!("{blocker}");
        }
    }
    Ok(u8::from(manifest.result.blocker.is_some()))
}
#[derive(Subcommand)]
enum AuthAction {
    /// Save a 64-hex bearer token to ~/.local/share/rupicola/auth-token (0600).
    SetToken {
        /// Bearer token (omit to read from stdin).
        #[arg(long)]
        token: Option<String>,
    },
}

#[expect(
    clippy::too_many_lines,
    reason = "the top-level CLI dispatcher is one exhaustive match on subcommands"
)]
fn run(cli: Cli) -> Result<u8, String> {
    match cli.command {
        Command::Mcp => mcp::serve().map(|()| 0).map_err(|e| e.to_string()),
        Command::McpBridge {
            server_url,
            project,
        } => mcp_bridge::serve(&server_url, project.as_deref()).map(|()| 0),
        Command::Auth {
            action: AuthAction::SetToken { token },
        } => mcp_bridge::set_token(token).map(|()| 0),
        Command::Capture {
            scenario: path,
            out,
            playwright_module,
            threshold,
            video,
            json: output_json,
        } => {
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
        Command::CaptureCosmic {
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
        } => {
            let (out, project_id, project_label) = if let Some(out) = out {
                (
                    out,
                    project_id.ok_or("--project-id is required with --out")?,
                    project_label.ok_or("--project-label is required with --out")?,
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
        Command::CaptureReview {
            manifest,
            note,
            evidence,
        } => {
            scenario::review(&manifest, note, evidence)?;
            println!("{}: inspection recorded", manifest.display());
            Ok(0)
        }
        Command::Setup { print_unit } => {
            service::setup(print_unit)?;
            Ok(0)
        }
        Command::Link { project } => {
            service::link(project)?;
            Ok(0)
        }
        Command::Design { action } => {
            design::run(action)?;
            Ok(0)
        }
        Command::Propose { action } => {
            propose::run(action)?;
            Ok(0)
        }
        Command::Problem { action } => pipeline::run_problem(action),
        Command::Fix { action } => pipeline::run_fix(action),
        Command::Storage { action } => pipeline::run_storage(action),
        Command::Probe {
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
        } => {
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
                println!(
                    "{}",
                    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
                );
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
                1
            } else if summary.findings_count > 0 {
                2
            } else {
                0
            })
        }
        Command::Doctor => {
            service::doctor()?;
            Ok(0)
        }
        Command::Serve { options } => {
            let status = std::process::Command::new("rupicola-panel")
                .args(options)
                .status()
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        "install the optional panel with cargo install --path panel --locked and add it to PATH".into()
                    } else {
                        format!("start rupicola-panel: {error}")
                    }
                })?;
            Ok(status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(1))
        }
        Command::Discover {
            path,
            json: output_json,
        } => {
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
        Command::Scan {
            path,
            framework,
            include_tests,
            json: output_json,
        } => {
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
                1
            } else if !result.findings.is_empty() {
                2
            } else if result.parsed_files == 0 || !result.unsupported_files.is_empty() {
                3
            } else {
                0
            })
        }
        Command::Diff {
            before,
            after,
            threshold,
            out,
            json: output_json,
        } => {
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
                    report.changed_ratio * 100.0,
                    report.mean_abs_delta,
                    report.max_delta,
                    report.bbox
                );
            }
            Ok(if report.dimension_mismatch || report.changed_pixels > 0 {
                2
            } else {
                0
            })
        }
        Command::Frames {
            dir,
            threshold,
            json: output_json,
        } => {
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
                    report.total_changed_ratio * 100.0
                );
                for note in &report.notes {
                    println!("note: {note}");
                }
            }
            Ok(
                if report.stalls.is_empty()
                    && report.jumps.is_empty()
                    && report.first_change.is_some()
                {
                    0
                } else {
                    2
                },
            )
        }
        Command::Video {
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
        } => {
            let options = capture::VideoOptions {
                fps,
                crf,
                preset,
                ..capture::VideoOptions::default()
            };
            let report = if raw {
                if dir.is_some() {
                    return Err("cannot combine a PNG directory with --raw".into());
                }
                let width = width.ok_or("--width is required with --raw")?;
                let height = height.ok_or("--height is required with --raw")?;
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
                let dir = dir.ok_or("provide a PNG directory, or pass --raw to read from stdin")?;
                if save_frames.is_some() {
                    return Err("--save-frames is only valid with --raw".into());
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
            Ok(0)
        }
        Command::Palette {
            image,
            colors,
            json: output_json,
        } => {
            let report = visual::palette(&visual::load(&image)?, colors);
            if output_json {
                json(&report)?;
            } else {
                for c in &report.colors {
                    println!("{} {:.1}%", c.hex, c.share * 100.0);
                }
                println!("suggested: {:?}", report.suggested);
            }
            Ok(0)
        }
        Command::Sheet {
            dir,
            out,
            cols,
            thumb,
        } => {
            let files = visual::list_pngs(&dir)?;
            visual::save(&out, &visual::sheet(&files, cols, thumb)?)?;
            println!("{} images, in order:", files.len());
            for f in &files {
                println!("{}", f.display());
            }
            Ok(0)
        }
        Command::Matrix {
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
        } => {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
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
                return Err(format!(
                    "{} combinations exceed --max {max}; narrow the lists or raise the limit",
                    entries.len()
                ));
            }
            if !dry_run {
                visual::matrix_run(
                    &mut entries,
                    &cmd,
                    std::time::Duration::from_secs(timeout_secs),
                );
                std::fs::write(
                    out.join("manifest.json"),
                    serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
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
        Command::Audit {
            tree,
            json: output_json,
        } => {
            let text =
                std::fs::read_to_string(&tree).map_err(|e| format!("{}: {e}", tree.display()))?;
            let parsed: visual::Tree =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", tree.display()))?;
            let findings = visual::audit(&parsed);
            if output_json {
                json(&findings)?;
            } else {
                for f in &findings {
                    println!("{} {}: {}", f.rule, f.node, f.message);
                }
                println!("{} nodes, {} findings", parsed.nodes.len(), findings.len());
            }
            Ok(if findings.is_empty() { 0 } else { 2 })
        }
        Command::Check {
            path,
            json: output_json,
        } => {
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
        Command::Rules { json: output_json } => {
            if output_json {
                json(&scan::RULES)?;
            } else {
                for (id, description) in scan::RULES {
                    println!("{id}: {description}");
                }
            }
            Ok(0)
        }
        Command::Modules { json: output_json } => {
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
            Ok(0)
        }
    }
}
fn main() -> ExitCode {
    let builder = std::thread::Builder::new()
        .name("rupicola-main".to_string())
        .stack_size(8 * 1024 * 1024);
    let handle = builder
        .spawn(|| run(Cli::parse()))
        .unwrap_or_else(|e| panic!("failed to spawn main thread: {e}"));
    match handle.join() {
        Ok(Ok(code)) => ExitCode::from(code),
        Ok(Err(message)) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
        Err(_) => ExitCode::FAILURE,
    }
}
