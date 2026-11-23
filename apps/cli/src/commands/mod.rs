//! Subcommands: each takes its own argument struct and runs itself.

mod account;
mod analysis;
mod capture;
mod captures;
mod client;
mod design;
mod fixes;
mod images;
mod panel;
mod problems;
mod projects;
mod remote;

use crate::{AuthAction, EXIT_CLEAN, Result, service};
use account::{LoginArgs, Logout, WhoamiArgs};
use analysis::{AuditArgs, CheckArgs, DiscoverArgs, ModulesArgs, RulesArgs, ScanArgs};
use capture::CaptureArgs;
use captures::CapturesArgs;
use clap::Subcommand;
use design::DesignArgs;
use fixes::FixesArgs;
use images::{DiffArgs, FramesArgs, PaletteArgs, SheetArgs, VideoArgs};
use panel::ServeArgs;
use problems::ProblemsArgs;
use projects::ProjectsArgs;
use remote::{McpBridgeArgs, PublishArgs};

#[derive(Subcommand)]
pub enum Command {
    /// Bridge MCP stdio to a Rupicola backend using the stored bearer token.
    McpBridge(McpBridgeArgs),
    /// Upload a capture directory to a Rupicola backend.
    Publish(PublishArgs),
    /// Sign in to a Rupicola backend and store a token for this machine.
    Login(LoginArgs),
    /// Forget the stored token and backend.
    Logout,
    /// Show who the stored token acts as.
    Whoami(WhoamiArgs),
    /// List and create projects.
    Projects(ProjectsArgs),
    /// Read a project's captures and note what you saw.
    Captures(CapturesArgs),
    /// Open, triage and discuss a project's problems.
    Problems(ProblemsArgs),
    /// Record and review a project's fixes.
    Fixes(FixesArgs),
    /// Read and record a project's design records.
    Design(DesignArgs),
    /// Manage the stored token by hand.
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
    /// Link the current checkout to a project in the local panel.
    Link { project: String },
    /// Serve project evidence using the optional rupicola-panel executable.
    Serve(ServeArgs),
    /// Assemble PNG frames or a raw RGBA/BGRA stdin stream into a constant-framerate MP4 via ffmpeg.
    Video(VideoArgs),
    /// Extract a dominant-color palette and suggested roles from a PNG.
    Palette(PaletteArgs),
    /// Combine PNGs from a directory into one contact-sheet image.
    Sheet(SheetArgs),
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
            Self::McpBridge(args) => args.run(),
            Self::Publish(args) => args.run(),
            Self::Login(args) => args.run(),
            Self::Logout => Logout::run(),
            Self::Whoami(args) => args.run(),
            Self::Projects(args) => args.run(),
            Self::Captures(args) => args.run(),
            Self::Problems(args) => args.run(),
            Self::Fixes(args) => args.run(),
            Self::Design(args) => args.run(),
            Self::Auth {
                action: AuthAction::SetToken { token },
            } => account::set_token(token).map(|()| EXIT_CLEAN),
            Self::Discover(args) => args.run(),
            Self::Scan(args) => args.run(),
            Self::Check(args) => args.run(),
            Self::Diff(args) => args.run(),
            Self::Frames(args) => args.run(),
            Self::Capture(args) => args.run(),
            Self::Link { project } => {
                service::link(project)?;
                Ok(EXIT_CLEAN)
            }
            Self::Serve(args) => args.run(),
            Self::Video(args) => args.run(),
            Self::Palette(args) => args.run(),
            Self::Sheet(args) => args.run(),
            Self::Audit(args) => args.run(),
            Self::Rules(args) => args.run(),
            Self::Modules(args) => args.run(),
        }
    }
}
