//! Commands that talk to a Rupicola backend.

use crate::{
    EXIT_CLEAN, Result, block_on,
    mcp_bridge::{self, Remote, SERVER_URL_ENV},
    publish::Bundle,
};
use anyhow::bail;
use rupicola_client::Api;
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct McpBridgeArgs {
    /// Base URL of the Rupicola backend (a trailing /mcp is accepted).
    #[arg(long, env = SERVER_URL_ENV)]
    server_url: String,
}

impl McpBridgeArgs {
    pub(super) fn run(self) -> Result<u8> {
        block_on(mcp_bridge::serve(&self.server_url))??;
        Ok(EXIT_CLEAN)
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
        block_on(async {
            let bundle = Bundle::open(&self.dir).await?;
            let project = self.project.as_deref().unwrap_or_else(|| bundle.project());
            if project.is_empty() {
                bail!("the manifest names no project; pass --project");
            }
            let remote = Remote::connect(&self.server_url)?;
            let capture = remote
                .api
                .publish_capture(&remote.token, project, &bundle.describe(&remote).await?)
                .await?;
            println!("Published capture {} to project {project}", capture.id);
            Ok(EXIT_CLEAN)
        })?
    }
}
