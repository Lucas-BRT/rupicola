use crate::service;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, BufRead, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

fn validate_token(raw: &str) -> Result<String, String> {
    let token = raw.trim();
    if token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(token.to_owned())
    } else {
        Err("Rupicola auth token must be 64 hexadecimal characters".into())
    }
}

pub fn token_path() -> Result<PathBuf, String> {
    Ok(service::data()?.join("auth-token"))
}

pub fn resolve_token() -> Result<String, String> {
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
    Err(format!(
        "Cannot read {} and RUPICOLA_MCP_TOKEN is unset; run `rupicola auth set-token`",
        path.display()
    ))
}

pub fn set_token(token: Option<String>) -> Result<(), String> {
    let raw = if let Some(value) = token {
        value
    } else {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("Read token from stdin: {e}"))?;
        buf
    };
    let token = validate_token(&raw)?;
    let path = token_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Create {}: {e}", parent.display()))?;
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
        .map_err(|e| format!("Write {}: {e}", path.display()))?;
    file.write_all(token.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|e| format!("Write {}: {e}", path.display()))?;
    println!("Saved token to {}", path.display());
    Ok(())
}

fn endpoint_url(server_url: &str, project: Option<&str>) -> String {
    let base = server_url.trim_end_matches('/');
    let base = if base.ends_with("/mcp") || base.contains("/mcp/") {
        base.to_owned()
    } else {
        format!("{base}/mcp")
    };
    match project {
        Some(project) if !base.contains("/mcp/") => format!("{base}/{project}"),
        _ => base,
    }
}

fn forward(url: &str, token: &str, payload: &str) -> Result<(u16, String), String> {
    let mut child = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "30",
            "-w",
            "\n%{http_code}",
            "-X",
            "POST",
            url,
            "-H",
            &format!("Authorization: Bearer {token}"),
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Spawn curl: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload.as_bytes())
            .map_err(|e| format!("Write request to curl: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Wait for curl: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl failed: {}", stderr.trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some((body, code_str)) = stdout.trim_end_matches(['\r', '\n']).rsplit_once('\n') else {
        let code: u16 = stdout.trim().parse().unwrap_or(0);
        return Ok((code, String::new()));
    };
    let code: u16 = code_str.trim().parse().unwrap_or(0);
    Ok((code, body.to_owned()))
}

pub fn serve(server_url: &str, project: Option<&str>) -> Result<(), String> {
    let token = resolve_token()?;
    let url = endpoint_url(server_url, project);
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req_id = serde_json::from_str::<Value>(trimmed)
            .ok()
            .and_then(|v| v.get("id").cloned());
        match forward(&url, &token, trimmed) {
            Ok((200, body)) if !body.trim().is_empty() => {
                writeln!(stdout, "{}", body.trim()).map_err(|e| e.to_string())?;
                stdout.flush().map_err(|e| e.to_string())?;
            }
            Ok((202 | 204, _)) => {}
            Ok((code, body)) => {
                if let Some(id) = req_id {
                    let msg = if body.trim().is_empty() {
                        format!("Remote MCP returned HTTP {code}")
                    } else {
                        format!("Remote MCP returned HTTP {code}: {}", body.trim())
                    };
                    let err = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32001, "message": msg}});
                    writeln!(stdout, "{err}").map_err(|e| e.to_string())?;
                    stdout.flush().map_err(|e| e.to_string())?;
                }
            }
            Err(err_msg) => {
                if let Some(id) = req_id {
                    let err = json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": err_msg}});
                    writeln!(stdout, "{err}").map_err(|e| e.to_string())?;
                    stdout.flush().map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}
