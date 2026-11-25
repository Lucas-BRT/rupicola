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
    pub(crate) fn run(self) -> Result<u8> {
        block_on(mcp_bridge::serve(self.server.connect()?))??;
        Ok(EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct PublishArgs {
    /// Capture directory holding record.json and its member files.
    dir: PathBuf,
    /// Project to publish to; defaults to the project named in the record.
    #[arg(long)]
    project: Option<String>,
    #[command(flatten)]
    server: Server,
}

impl PublishArgs {
    pub(crate) fn run(self) -> Result<u8> {
        block_on(async {
            let bundle = Bundle::open(&self.dir).await?;
            let project = self.project.as_deref().unwrap_or_else(|| bundle.project());
            if project.is_empty() {
                bail!("the record names no project; pass --project");
            }
            let remote = self.server.connect()?;
            let record = bundle.upload_and_record(&remote, project).await?;
            let capture = remote
                .api
                .publish_record(&remote.token, project, &record.id, &record)
                .await?;
            println!("Published capture {} to project {project}", capture.id);
            Ok(EXIT_CLEAN)
        })?
    }
}
