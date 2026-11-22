mod commands;
use commands::Command;
mod design;
mod mcp;
mod mcp_bridge;
mod pipeline;
mod probe;
mod propose;
mod publish;
mod service;
use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use rupicola_core::{
    FRAMEWORKS, TOPICS, capture, check, design_record, discover, fix_record, problem_record,
    proposal_record, scan, scenario, storage, visual,
};
use std::process::ExitCode;

/// Process exit codes, documented in the README: success with nothing to
/// report, an error or diagnostic, findings that need review, and no supported
/// analysis.
const EXIT_CLEAN: u8 = 0;
const EXIT_ERROR: u8 = 1;
const EXIT_FINDINGS: u8 = 2;
const EXIT_UNSUPPORTED: u8 = 3;
/// Pages a probe crawls unless told otherwise.
const DEFAULT_MAX_PAGES: usize = 20;
/// Colors a palette reports unless told otherwise.
const DEFAULT_PALETTE_COLORS: usize = 8;
/// Contact sheet layout unless told otherwise.
const DEFAULT_SHEET_COLUMNS: usize = 4;
const DEFAULT_SHEET_THUMB_WIDTH: u32 = 320;
/// Desktop capture matrix: most combinations run, and seconds each may take.
const DEFAULT_MATRIX_MAX_RUNS: usize = 48;
const DEFAULT_MATRIX_TIMEOUT_SECS: u64 = 60;
/// Ratios are printed as percentages.
const PERCENT: f64 = 100.0;
/// The CLI runs on a thread with a larger stack: the default main-thread stack
/// overflowed on Windows.
const MAIN_STACK_BYTES: usize = 8 * 1024 * 1024;

#[derive(Parser)]
#[command(
    version,
    about = "Rust-native tools for AI-assisted interface development"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
/// Runs `future` to completion on a runtime built for the call; the CLI is short-lived and
/// only its network commands need one.
fn block_on<F: std::future::Future>(future: F) -> Result<F::Output> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(future))
}
fn json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
fn print_capture(
    manifest: &scenario::Manifest,
    out: &std::path::Path,
    output_json: bool,
) -> Result<u8> {
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

fn main() -> ExitCode {
    let builder = std::thread::Builder::new()
        .name("rupicola-main".to_string())
        .stack_size(MAIN_STACK_BYTES);
    let handle = builder
        .spawn(|| Cli::parse().command.run())
        .unwrap_or_else(|e| panic!("failed to spawn main thread: {e}"));
    match handle.join() {
        Ok(Ok(code)) => ExitCode::from(code),
        Ok(Err(error)) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
        Err(_) => ExitCode::FAILURE,
    }
}
