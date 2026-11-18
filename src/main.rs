use clap::{Parser, Subcommand};
use ferris_atelier::{FRAMEWORKS, TOPICS, check, discover, scan, visual};
use std::{path::PathBuf, process::ExitCode};

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
        /// Shell command; it receives FERRIS_OUT (PNG path), FERRIS_STATE, FERRIS_THEME, FERRIS_SCALE,
        /// FERRIS_TEXT_SCALE, FERRIS_SIZE, FERRIS_WIDTH, FERRIS_HEIGHT and FERRIS_LOCALE.
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
fn run(cli: Cli) -> Result<u8, String> {
    match cli.command {
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
            Ok(if result.diagnostics.is_empty() { 0 } else { 1 })
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
            Ok(if !dry_run && entries.iter().any(|e| !e.ok) {
                1
            } else {
                0
            })
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
            Ok(if result.diagnostics.is_empty() { 0 } else { 1 })
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
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
