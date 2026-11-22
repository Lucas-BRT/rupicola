//! Commands that talk to a Rupicola backend.

use crate::{EXIT_CLEAN, Result, api::Client, mcp_bridge, publish::Bundle};
use anyhow::bail;
use std::path::PathBuf;

/// Environment variable naming the backend, so no address is baked into the binary.
const SERVER_URL_ENV: &str = "RUPICOLA_SERVER_URL";

#[derive(clap::Args)]
pub struct McpBridgeArgs {
    /// Base URL of the Rupicola backend (a trailing /mcp is accepted).
    #[arg(long, env = SERVER_URL_ENV)]
    server_url: String,
}

impl McpBridgeArgs {
    pub(super) fn run(self) -> Result<u8> {
        mcp_bridge::serve(&self.server_url).map(|()| EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct PublishArgs {
    /// Capture directory holding manifest.json and its media files.
    dir: PathBuf,
    /// Project to publish to; defaults to the project named in the manifest.
    #[arg(long)]
    project: Option<String>,
    /// Base URL of the Rupicola backend.
    #[arg(long, env = SERVER_URL_ENV)]
    server_url: String,
}

impl PublishArgs {
    pub(super) fn run(self) -> Result<u8> {
        let bundle = Bundle::open(&self.dir)?;
        let project = self.project.as_deref().unwrap_or_else(|| bundle.project());
        if project.is_empty() {
            bail!("the manifest names no project; pass --project");
        }
        let client = Client::connect(&self.server_url)?;
        let capture = client.publish_capture(project, &bundle.describe(&client)?)?;
        println!("Published capture {} to project {project}", capture.id);
        Ok(EXIT_CLEAN)
    }
}
