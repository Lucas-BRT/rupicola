//! Commands that talk to a Rupicola backend.

use crate::{EXIT_CLEAN, Result, block_on, mcp_bridge, publish::Bundle, session::Server};
use anyhow::bail;
use rupicola_client::Api;
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct McpBridgeArgs {
    #[command(flatten)]
    server: Server,
}

impl McpBridgeArgs {
    pub(super) fn run(self) -> Result<u8> {
        block_on(mcp_bridge::serve(self.server.connect()?))??;
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
    #[command(flatten)]
    server: Server,
}

impl PublishArgs {
    pub(super) fn run(self) -> Result<u8> {
        block_on(async {
            let bundle = Bundle::open(&self.dir).await?;
            let project = self.project.as_deref().unwrap_or_else(|| bundle.project());
            if project.is_empty() {
                bail!("the manifest names no project; pass --project");
            }
            let remote = self.server.connect()?;
            let capture = remote
                .api
                .publish_capture(&remote.token, project, &bundle.describe(&remote).await?)
                .await?;
            println!("Published capture {} to project {project}", capture.id);
            Ok(EXIT_CLEAN)
        })?
    }
}
