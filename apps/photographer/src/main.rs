//! `rupicola-photographer`: standalone capture runner binary.

mod capture;
mod config;
mod preview;
mod runner;
mod worktree;

use anyhow::Result;
use capture::ProtocolCapturer;
use clap::Parser;
use rupicola_client::HttpApi;
use std::time::Duration;

const DEFAULT_POLL_SECS: u64 = 5;

#[derive(Parser)]
#[command(
    name = "rupicola-photographer",
    version,
    about = "Rupicola capture runner: claims jobs, builds worktrees, captures matrices, and uploads records"
)]
struct Args {
    /// Claim and run at most one job, then exit.
    #[arg(long)]
    once: bool,
    /// Seconds to wait between polling for queued jobs when idle.
    #[arg(long, default_value_t = DEFAULT_POLL_SECS)]
    poll_secs: u64,
    /// Base URL of the Rupicola backend (defaults to `RUPICOLA_SERVER_URL` or `rupicola login`).
    #[arg(long)]
    server_url: Option<String>,
    /// Bearer token (person or project device token; defaults to `RUPICOLA_RUNNER_TOKEN` or stored token).
    #[arg(long)]
    token: Option<String>,
    /// Project ID to claim jobs for (defaults to `.rupicola/project.toml` or Git checkout hash).
    #[arg(long)]
    project: Option<String>,
    /// Override `[preview].command` from `.rupicola/project.toml`.
    #[arg(long)]
    preview_command: Option<String>,
    /// Override `[preview].ready` from `.rupicola/project.toml`.
    #[arg(long)]
    preview_ready: Option<String>,
    /// Override `[preview].timeout_secs` from `.rupicola/project.toml`.
    #[arg(long)]
    preview_timeout_secs: Option<u64>,
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let args = Args::parse();
    let project_cfg = config::load_project_config()?;
    let project = config::resolve_project(args.project.as_deref(), project_cfg.as_ref())?;
    let server_url = config::resolve_server_url(args.server_url.as_deref())?;
    let token = config::resolve_token(args.token.as_deref())?;
    let repo_root = config::repo_root()?;
    let preview = config::resolve_preview(
        project_cfg.as_ref(),
        args.preview_command,
        args.preview_ready,
        args.preview_timeout_secs,
    )?;

    let api = HttpApi::new(&server_url)?;
    let capturer = ProtocolCapturer::discover().await?;
    runner::run_loop(
        &api,
        &token,
        &project,
        &repo_root,
        &preview,
        &capturer,
        args.once,
        Duration::from_secs(args.poll_secs.max(1)),
    )
    .await
}
