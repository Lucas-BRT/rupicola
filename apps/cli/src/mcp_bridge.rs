use crate::{api::Client, service};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, BufRead, Read, Write},
    path::PathBuf,
};

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

pub fn serve(server_url: &str) -> Result<()> {
    let client = Client::connect(server_url)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req_id = serde_json::from_str::<Value>(trimmed)
            .ok()
            .and_then(|v| v.get("id").cloned());
        match client.mcp(trimmed) {
            Ok((HTTP_OK, body)) if !body.trim().is_empty() => {
                writeln!(stdout, "{}", body.trim())?;
                stdout.flush()?;
            }
            Ok((HTTP_ACCEPTED | HTTP_NO_CONTENT, _)) => {}
            Ok((code, body)) => {
                if let Some(id) = req_id {
                    let msg = if body.trim().is_empty() {
                        format!("Remote MCP returned HTTP {code}")
                    } else {
                        format!("Remote MCP returned HTTP {code}: {}", body.trim())
                    };
                    let err = json!({"jsonrpc": "2.0", "id": id, "error": {"code": crate::mcp::JSONRPC_HTTP_ERROR, "message": msg}});
                    writeln!(stdout, "{err}")?;
                    stdout.flush()?;
                }
            }
            Err(err_msg) => {
                if let Some(id) = req_id {
                    let err = json!({"jsonrpc": "2.0", "id": id, "error": {"code": crate::mcp::JSONRPC_TRANSPORT_ERROR, "message": format!("{err_msg:#}")}});
                    writeln!(stdout, "{err}")?;
                    stdout.flush()?;
                }
            }
        }
    }
    Ok(())
}
