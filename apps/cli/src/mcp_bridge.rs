use crate::service;
use anyhow::{Context, Result, bail};
use rupicola_client::{Api, HttpApi, McpReply};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, Read, Write},
    path::PathBuf,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Environment variable naming the backend, so no address is baked into the binary.
pub const SERVER_URL_ENV: &str = "RUPICOLA_SERVER_URL";
/// Path of the MCP endpoint, which clients may be configured with instead of the base URL.
const MCP_SUFFIX: &str = "/mcp";

/// A backend and the stored token that acts for this machine.
pub struct Remote {
    pub api: HttpApi,
    pub token: String,
}

impl Remote {
    /// A connection to `server_url` (the base URL, or its `/mcp` endpoint), using the token
    /// from [`resolve_token`].
    ///
    /// # Errors
    ///
    /// Fails when no token is stored or the HTTP client cannot be built.
    pub fn connect(server_url: &str) -> Result<Self> {
        let base = server_url.trim_end_matches('/');
        let base = base.strip_suffix(MCP_SUFFIX).unwrap_or(base);
        Ok(Self {
            api: HttpApi::new(base)?,
            token: resolve_token()?,
        })
    }
}

/// Length of an auth token: 32 random bytes written as hex.
const TOKEN_HEX_LEN: usize = 64;
/// HTTP statuses the remote MCP endpoint answers with.
const HTTP_OK: u16 = 200;
const HTTP_ACCEPTED: u16 = 202;
const HTTP_NO_CONTENT: u16 = 204;

fn validate_token(raw: &str) -> Result<String> {
    let token = raw.trim();
    if token.len() == TOKEN_HEX_LEN && token.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(token.to_owned())
    } else {
        bail!("Rupicola auth token must be {TOKEN_HEX_LEN} hexadecimal characters");
    }
}

pub fn token_path() -> Result<PathBuf> {
    Ok(service::data()?.join("auth-token"))
}

pub fn resolve_token() -> Result<String> {
    let path = token_path()?;
    if let Ok(raw) = fs::read_to_string(&path)
        && !raw.trim().is_empty()
    {
        return validate_token(&raw);
    }
    if let Ok(token) = env::var("RUPICOLA_MCP_TOKEN")
        && !token.trim().is_empty()
    {
        return validate_token(&token);
    }
    bail!(
        "Cannot read {} and RUPICOLA_MCP_TOKEN is unset; run `rupicola auth set-token`",
        path.display()
    );
}

pub fn set_token(token: Option<String>) -> Result<()> {
    let raw = if let Some(value) = token {
        value
    } else {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .context("Read token from stdin")?;
        buf
    };
    let token = validate_token(&raw)?;
    let path = token_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Create {}", parent.display()))?;
    }
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&path)
        .with_context(|| format!("Write {}", path.display()))?;
    file.write_all(token.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .with_context(|| format!("Write {}", path.display()))?;
    println!("Saved token to {}", path.display());
    Ok(())
}

pub async fn serve(server_url: &str) -> Result<()> {
    let remote = Remote::connect(server_url)?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req_id = serde_json::from_str::<Value>(trimmed)
            .ok()
            .and_then(|v| v.get("id").cloned());
        let answer = match remote.api.mcp(&remote.token, trimmed).await {
            Ok(McpReply {
                status: HTTP_OK,
                body,
            }) if !body.trim().is_empty() => Some(body.trim().to_owned()),
            Ok(McpReply {
                status: HTTP_ACCEPTED | HTTP_NO_CONTENT,
                ..
            }) => None,
            Ok(McpReply { status, body }) => req_id.map(|id| {
                let msg = if body.trim().is_empty() {
                    format!("Remote MCP returned HTTP {status}")
                } else {
                    format!("Remote MCP returned HTTP {status}: {}", body.trim())
                };
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": crate::mcp::JSONRPC_HTTP_ERROR, "message": msg}}).to_string()
            }),
            Err(error) => req_id.map(|id| {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": crate::mcp::JSONRPC_TRANSPORT_ERROR, "message": error.to_string()}}).to_string()
            }),
        };
        if let Some(answer) = answer {
            stdout.write_all(answer.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}
