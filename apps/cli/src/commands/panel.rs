//! The command that runs the panel.

use crate::{Result, anyhow};
use std::ffi::OsString;

#[derive(clap::Args)]
pub struct ServeArgs {
    /// Options forwarded to rupicola-panel, including --server-url and --port.
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
