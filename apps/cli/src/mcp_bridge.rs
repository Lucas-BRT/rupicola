use crate::session::Remote;
use anyhow::Result;
use rupicola_client::{Api, McpReply};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Server-defined JSON-RPC error codes the bridge uses for transport and HTTP failures.
const JSONRPC_TRANSPORT_ERROR: i64 = -32000;
const JSONRPC_HTTP_ERROR: i64 = -32001;

/// HTTP statuses the remote MCP endpoint answers with.
const HTTP_OK: u16 = 200;
const HTTP_ACCEPTED: u16 = 202;
const HTTP_NO_CONTENT: u16 = 204;

/// Relays the MCP messages on stdin to the backend, and its answers to stdout.
///
/// # Errors
///
/// Fails when stdin or stdout cannot be used.
pub async fn serve(remote: Remote) -> Result<()> {
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
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": JSONRPC_HTTP_ERROR, "message": msg}}).to_string()
            }),
            Err(error) => req_id.map(|id| {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": JSONRPC_TRANSPORT_ERROR, "message": error.to_string()}}).to_string()
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
