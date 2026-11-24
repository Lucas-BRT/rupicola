//! Commands that sign this machine in to a backend and out again.

use crate::{
    EXIT_CLEAN, Result, block_on, json,
    session::{Server, Store},
};
use anyhow::{Context, bail};
use rupicola_api::{Credentials, NewCredential};
use rupicola_client::{Api, HttpApi};
use std::io::{self, Read, Write};

/// What the machine token is called in the person's list of sessions and tokens.
const DEFAULT_LABEL: &str = "cli";

#[derive(clap::Args)]
pub struct LoginArgs {
    #[command(flatten)]
    server: Server,
    /// Username; asked for when omitted.
    #[arg(long)]
    handle: Option<String>,
    /// Read the password from the first line of stdin instead of asking for it.
    #[arg(long)]
    password_stdin: bool,
    /// Name of the token in the account's list of sessions and tokens.
    #[arg(long, default_value = DEFAULT_LABEL)]
    label: String,
}

impl LoginArgs {
    /// Signs in with the password, asks the backend for a machine token, stores it with the
    /// backend's address, and ends the session the password opened.
    pub(crate) fn run(self) -> Result<u8> {
        let url = self.server.require_url()?;
        let handle = match self.handle {
            Some(handle) => handle,
            None => Self::ask("Username: ")?,
        };
        let password = if self.password_stdin {
            Self::line_from_stdin()?
        } else {
            rpassword::prompt_password("Password: ").context("Read the password")?
        };
        block_on(async {
            let api = HttpApi::new(&url)?;
            let session = api
                .sign_in(&Credentials {
                    handle: handle.clone(),
                    password,
                })
                .await?;
            let issued = api
                .create_token(
                    &session.token,
                    &NewCredential {
                        label: self.label.clone(),
                    },
                )
                .await;
            // The session was only a way to obtain the token, so it ends either way.
            let _ = api.sign_out(&session.token).await;
            let issued = issued?;
            let store = Store::open()?;
            store.save_token(&issued.token)?;
            store.save_server_url(&url)?;
            println!(
                "Signed in to {url} as {handle}; token saved to {}",
                store.token_path().display()
            );
            Ok(EXIT_CLEAN)
        })?
    }

    fn ask(prompt: &str) -> Result<String> {
        eprint!("{prompt}");
        io::stderr().flush()?;
        Self::line_from_stdin()
    }

    fn line_from_stdin() -> Result<String> {
        let mut line = String::new();
        io::stdin().read_line(&mut line).context("Read stdin")?;
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    }
}

/// The `logout` command, which takes no arguments.
pub struct Logout;

impl Logout {
    /// Forgets the stored token and backend. The token stays valid on the backend until it
    /// is revoked there, because only a signed-in session may revoke tokens.
    pub(crate) fn run() -> Result<u8> {
        if Store::open()?.clear()? {
            println!("Removed the stored token and backend.");
        } else {
            println!("No stored token.");
        }
        Ok(EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct WhoamiArgs {
    #[command(flatten)]
    server: Server,
    #[arg(long)]
    json: bool,
}

impl WhoamiArgs {
    pub(crate) fn run(self) -> Result<u8> {
        let url = self.server.require_url()?;
        block_on(async {
            let remote = self.server.connect()?;
            let me = remote.api.me(&remote.token).await?;
            if self.json {
                json(&me)?;
            } else {
                let name = me.handle.as_deref().unwrap_or("(no account)");
                let admin = if me.admin { ", administrator" } else { "" };
                println!("{name} on {url} ({}{admin})", me.kind);
            }
            Ok(EXIT_CLEAN)
        })?
    }
}

/// Stores a token the backend issued some other way.
///
/// # Errors
///
/// Fails when the token is malformed, unreadable from stdin, or cannot be stored.
pub fn set_token(token: Option<String>) -> Result<()> {
    let raw = if let Some(value) = token {
        value
    } else {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Read token from stdin")?;
        buffer
    };
    if raw.trim().is_empty() {
        bail!("no token given");
    }
    let store = Store::open()?;
    store.save_token(&raw)?;
    println!("Saved token to {}", store.token_path().display());
    Ok(())
}
