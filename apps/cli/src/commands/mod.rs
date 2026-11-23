//! Subcommands: each takes its own argument struct and runs itself.

mod analysis;
mod capture;
mod images;
mod panel;
mod remote;

use crate::{AuthAction, EXIT_CLEAN, Result, design, mcp, mcp_bridge, pipeline, propose, service};
use analysis::{AuditArgs, CheckArgs, DiscoverArgs, ModulesArgs, RulesArgs, ScanArgs};
use capture::{CaptureArgs, CaptureReviewArgs, MatrixArgs};
use clap::Subcommand;
use images::{DiffArgs, FramesArgs, PaletteArgs, SheetArgs, VideoArgs};
use panel::{ServeArgs, SetupArgs};
use remote::{McpBridgeArgs, PublishArgs};

#[derive(Subcommand)]
pub enum Command {
    /// Serve Rupicola tools over MCP stdio.
    Mcp,
    /// Bridge MCP stdio to a Rupicola backend using the stored bearer token.
    McpBridge(McpBridgeArgs),
    /// Upload a capture directory to a Rupicola backend.
    Publish(PublishArgs),
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

impl Command {
    pub fn run(self) -> Result<u8> {
        match self {
            Self::Mcp => Ok(mcp::serve().map(|()| EXIT_CLEAN)?),
            Self::McpBridge(args) => args.run(),
            Self::Publish(args) => args.run(),
            Self::Auth {
                action: AuthAction::SetToken { token },
            } => mcp_bridge::set_token(token).map(|()| EXIT_CLEAN),
            Self::Discover(args) => args.run(),
            Self::Scan(args) => args.run(),
            Self::Check(args) => args.run(),
            Self::Diff(args) => args.run(),
            Self::Frames(args) => args.run(),
            Self::Capture(args) => args.run(),
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
