//! Commands that talk to the panel or run its service.

use crate::{EXIT_CLEAN, Result, anyhow, mcp_bridge, service};
use std::ffi::OsString;

#[derive(clap::Args)]
pub struct McpBridgeArgs {
    /// Base URL of the remote Rupicola panel or its /mcp endpoint.
    #[arg(long, default_value = "https://rupicola.lucasbrt.dev/mcp")]
    server_url: String,
    /// Optional project ID to scope the remote MCP session.
    #[arg(long)]
    project: Option<String>,
}

impl McpBridgeArgs {
    pub(super) fn run(self) -> Result<u8> {
        let McpBridgeArgs {
            server_url,
            project,
        } = self;
        mcp_bridge::serve(&server_url, project.as_deref()).map(|()| EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct SetupArgs {
    /// Print the systemd unit without modifying the system.
    #[arg(long)]
    print_unit: bool,
}

impl SetupArgs {
    pub(super) fn run(self) -> Result<u8> {
        let SetupArgs { print_unit } = self;
        service::setup(print_unit)?;
        Ok(EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct ServeArgs {
    /// Options forwarded to rupicola-panel, including --captures, --db and --port.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    options: Vec<OsString>,
}

impl ServeArgs {
    pub(super) fn run(self) -> Result<u8> {
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
