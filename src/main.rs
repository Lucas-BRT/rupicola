use clap::{Parser, Subcommand};
use ferris_atelier::{FRAMEWORKS, TOPICS, check, discover, scan};
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
