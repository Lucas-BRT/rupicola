//! The MCP stdio server: session marker and the JSON-RPC read loop.

use super::{
    JSONRPC_METHOD_NOT_FOUND, JSONRPC_PARSE_ERROR, call::tool, catalog::TOOLS, preview::Preview,
};
use crate::{proposal_record, service};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

struct SessionGuard(Option<PathBuf>);

impl SessionGuard {
    fn register() -> Self {
        let Ok(data) = service::data() else {
            return Self(None);
        };
        let path = data.join(proposal_record::MCP_SESSION_FILE);
        let started_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        let session = proposal_record::McpSession {
            pid: std::process::id(),
            project_id: service::project().ok().flatten().map(|p| p.0),
            client: None,
            started_at_ms,
        };
        if proposal_record::write_atomic(&path, &session).is_ok() {
            Self(Some(path))
        } else {
            Self(None)
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take()
            && proposal_record::read_json::<proposal_record::McpSession>(
                &path,
                proposal_record::MAX_SESSION_BYTES,
            )
            .is_ok_and(|s| s.pid == std::process::id())
        {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn serve() -> io::Result<()> {
    let _session = SessionGuard::register();
    let mut preview = Preview::default();
    let mut stdout = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let request: Value = if let Ok(value) = serde_json::from_str(&line) {
            value
        } else {
            writeln!(
                stdout,
                "{}",
                json!({"jsonrpc":"2.0","id":null,"error":{"code":JSONRPC_PARSE_ERROR,"message":"Parse error"}})
            )?;
            stdout.flush()?;
            continue;
        };
        let Some(id) = request.get("id") else {
            continue;
        };
        let params = &request["params"];
        let body = match request["method"].as_str() {
            Some("initialize") => {
                let version = params["protocolVersion"].as_str().unwrap_or("");
                let version = match version {
                    "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25" => version,
                    _ => "2025-11-25",
                };
                json!({"result":{"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"rupicola","version":env!("CARGO_PKG_VERSION")}}})
            }
            Some("ping") => json!({"result":{}}),
            Some("tools/list") => {
                let tools: Vec<_> = TOOLS.iter().map(|(name, description, required, optional)| {
                    let properties: serde_json::Map<_,_> = required.iter().chain(*optional).map(|key|
                        ((*key).into(), json!({"type":match *key {
                            "scenario" => "object",
                            "video" | "demo" => "boolean",
                            "x" | "y" | "timeout_secs" | "max_pages" | "pct" | "step" | "total_steps" => "integer",
                            "palette_hex" | "identified_tactics" | "target_requirements" => "array",
                            _ => "string"
                        }}))
                    ).collect();
                    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required}})
                }).collect();
                json!({"result":{"tools":tools}})
            }
            Some("tools/call") => {
                let result = tool(
                    params["name"].as_str().unwrap_or(""),
                    &params["arguments"],
                    &mut preview,
                )
                .unwrap_or_else(
                    |error| json!({"isError":true,"content":[{"type":"text","text":format!("{error:#}")}]}),
                );
                json!({"result":result})
            }
            _ => json!({"error":{"code":JSONRPC_METHOD_NOT_FOUND,"message":"Method not found"}}),
        };
        let mut reply = json!({"jsonrpc":"2.0","id":id});
        reply
            .as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        writeln!(stdout, "{reply}")?;
        stdout.flush()?;
    }
    Ok(())
}
