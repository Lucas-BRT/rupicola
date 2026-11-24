//! Subcommands: separated into offline static-analysis tools (`offline`) and backend
//! HTTP/MCP client commands (`remote`).

mod offline;
mod remote;

use crate::{AuthAction, EXIT_CLEAN, Result, service};
use clap::Subcommand;
use offline::{CheckArgs, DiscoverArgs, ModulesArgs, RulesArgs, ScanArgs, ServeArgs};
use remote::{
    CapturesArgs, DesignArgs, FixesArgs, LoginArgs, Logout, McpBridgeArgs, ProblemsArgs,
    ProjectsArgs, PublishArgs, WhoamiArgs,
};

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
    /// Link the current checkout to a project in the local panel.
    Link { project: String },
    /// Serve project evidence using the optional rupicola-panel executable.
    Serve(ServeArgs),
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
            } => remote::set_token(token).map(|()| EXIT_CLEAN),
            Self::Discover(args) => args.run(),
            Self::Scan(args) => args.run(),
            Self::Check(args) => args.run(),
            Self::Link { project } => {
                service::link(project)?;
                Ok(EXIT_CLEAN)
            }
            Self::Serve(args) => args.run(),
            Self::Rules(args) => args.run(),
            Self::Modules(args) => args.run(),
        }
    }
}
