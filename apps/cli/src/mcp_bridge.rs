use crate::publish::Bundle;
use crate::session::Remote;
use anyhow::{Context, Result, bail};
use rupicola_client::{McpReply, McpTransport, WorkspaceClient, upload_missing_blobs};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, path::Path};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Server-defined JSON-RPC error codes the bridge uses for transport and HTTP failures.
const JSONRPC_TRANSPORT_ERROR: i64 = -32000;
const JSONRPC_HTTP_ERROR: i64 = -32001;

/// HTTP statuses the remote MCP endpoint answers with.
const HTTP_OK: u16 = 200;
const HTTP_ACCEPTED: u16 = 202;
const HTTP_NO_CONTENT: u16 = 204;

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Pre-uploads local `file_path` members or a local `bundle_dir` for `capture_publish`
/// calls over binary HTTP so agents never need to shell out to `rupicola publish` or inline
/// multi-megabyte base64 strings.
async fn prepare_request(remote: &Remote, trimmed: &str) -> Result<String> {
    let Ok(mut msg) = serde_json::from_str::<Value>(trimmed) else {
        return Ok(trimmed.to_owned());
    };
    if msg.get("method").and_then(Value::as_str) != Some("tools/call") {
        return Ok(trimmed.to_owned());
    }
    let Some(params) = msg.get_mut("params").and_then(Value::as_object_mut) else {
        return Ok(trimmed.to_owned());
    };
    if params.get("name").and_then(Value::as_str) != Some("capture_publish") {
        return Ok(trimmed.to_owned());
    }
    let Some(args) = params.get_mut("arguments").and_then(Value::as_object_mut) else {
        return Ok(trimmed.to_owned());
    };

    if let Some(bundle_dir) = args
        .remove("bundle_dir")
        .and_then(|v| v.as_str().map(str::to_owned))
    {
        prepare_bundle_dir(remote, args, Path::new(&bundle_dir)).await?;
        return Ok(msg.to_string());
    }

    let Some(project) = args
        .get("project")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
    else {
        return Ok(trimmed.to_owned());
    };
    let Some(members) = args.get_mut("members").and_then(Value::as_array_mut) else {
        return Ok(trimmed.to_owned());
    };
    prepare_member_files(remote, &project, members).await?;
    Ok(msg.to_string())
}

async fn prepare_bundle_dir(
    remote: &Remote,
    args: &mut serde_json::Map<String, Value>,
    bundle_dir: &Path,
) -> Result<()> {
    let bundle = Bundle::open(bundle_dir).await?;
    let project = args
        .get("project")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| bundle.project())
        .to_owned();
    let record = bundle.upload_and_record(remote, &project).await?;
    args.insert("project".to_owned(), Value::String(project));
    args.entry("scenario".to_owned())
        .or_insert_with(|| Value::String(record.scenario.name.clone()));
    if let Some(rev) = record.revision {
        args.entry("revision".to_owned())
            .or_insert_with(|| Value::String(rev));
    }
    let members: Vec<Value> = record
        .members
        .into_iter()
        .map(|m| {
            json!({
                "id": m.owner,
                "role": m.role,
                "mime": m.mime,
                "path": m.path,
                "sha256": m.sha256,
                "bytes": m.bytes,
            })
        })
        .collect();
    args.insert("members".to_owned(), Value::Array(members));
    Ok(())
}

async fn prepare_member_files(remote: &Remote, project: &str, members: &mut [Value]) -> Result<()> {
    let mut staged: Vec<(String, Vec<u8>)> = Vec::new();
    for member in members.iter_mut() {
        let Some(obj) = member.as_object_mut() else {
            continue;
        };
        let Some(file_path) = obj
            .remove("file_path")
            .and_then(|v| v.as_str().map(str::to_owned))
        else {
            continue;
        };
        let path = Path::new(&file_path);
        let data = tokio::fs::read(path)
            .await
            .with_context(|| format!("read capture_publish file_path `{file_path}`"))?;
        if data.is_empty() {
            bail!("capture_publish file_path `{file_path}` is empty");
        }
        let sha = sha256_hex(&data);
        if !obj.contains_key("id")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            obj.insert("id".to_owned(), Value::String(stem.to_owned()));
        }
        if !obj.contains_key("path")
            && let Some(name) = path.file_name().and_then(|s| s.to_str())
        {
            let dir = match path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("mp4" | "webm") => "clips",
                _ => "checkpoints",
            };
            obj.insert("path".to_owned(), Value::String(format!("{dir}/{name}")));
        }
        obj.insert("sha256".to_owned(), Value::String(sha.clone()));
        obj.insert("bytes".to_owned(), json!(data.len()));
        staged.push((sha, data));
    }

    if staged.is_empty() {
        return Ok(());
    }
    let client = WorkspaceClient::new(remote.api.clone(), remote.token.clone());
    upload_missing_blobs(&client, project, staged)
        .await
        .context("upload capture_publish blobs")?;
    Ok(())
}

/// Executes local `offline_*` static-analysis tools directly against the local checkout
/// using `rupicola-core` so agents connected to a remote server still inspect local files.
fn try_local_offline_tool(trimmed: &str) -> Option<String> {
    let msg: Value = serde_json::from_str(trimmed).ok()?;
    if msg.get("method").and_then(Value::as_str) != Some("tools/call") {
        return None;
    }
    let id = msg.get("id")?.clone();
    let params = msg.get("params")?.as_object()?;
    let name = params.get("name").and_then(Value::as_str)?;
    let args = params
        .get("arguments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(".");
    let outcome: Result<Value> = match name {
        "offline_discover" => rupicola_core::discover::discover(Path::new(path))
            .and_then(|v| Ok(serde_json::to_value(v)?)),
        "offline_scan" => {
            let framework = args.get("framework").and_then(Value::as_str);
            let include_tests = args
                .get("include_tests")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            rupicola_core::scan::scan(Path::new(path), framework, include_tests)
                .and_then(|v| Ok(serde_json::to_value(v)?))
        }
        "offline_check" => {
            rupicola_core::check::check(Path::new(path)).and_then(|v| Ok(serde_json::to_value(v)?))
        }
        "offline_rules" => serde_json::to_value(rupicola_core::scan::RULES).map_err(Into::into),
        "offline_modules" => {
            let modules: Vec<String> = rupicola_core::FRAMEWORKS
                .iter()
                .map(|id| format!("reference/frameworks/{id}.md"))
                .chain(
                    rupicola_core::TOPICS
                        .iter()
                        .map(|id| format!("reference/ux/{id}.md")),
                )
                .collect();
            serde_json::to_value(modules).map_err(Into::into)
        }
        _ => return None,
    };
    let (text, is_error) = match outcome {
        Ok(body) => (body.to_string(), false),
        Err(err) => (err.to_string(), true),
    };
    Some(
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"content": [{"type": "text", "text": text}], "isError": is_error}
        })
        .to_string(),
    )
}

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
        if let Some(local_reply) = try_local_offline_tool(trimmed) {
            stdout.write_all(local_reply.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
            continue;
        }
        let req_id = serde_json::from_str::<Value>(trimmed)
            .ok()
            .and_then(|v| v.get("id").cloned());
        let prepared = match prepare_request(&remote, trimmed).await {
            Ok(body) => body,
            Err(error) => {
                if let Some(id) = req_id {
                    let reply = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": JSONRPC_TRANSPORT_ERROR, "message": error.to_string()}
                    })
                    .to_string();
                    stdout.write_all(reply.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                }
                continue;
            }
        };
        let transport = WorkspaceClient::new(remote.api.clone(), remote.token.clone());
        let answer = match transport.send(&prepared).await {
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

#[cfg(test)]
mod tests {
    use super::{sha256_hex, try_local_offline_tool};
    use serde_json::{Value, json};

    #[test]
    fn sha256_hex_matches_known_digest() {
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn local_offline_tools_execute_and_remote_tools_pass_through() {
        let rules_req = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {"name": "offline_rules", "arguments": {}}
        })
        .to_string();
        let reply_raw = try_local_offline_tool(&rules_req).expect("offline_rules handled locally");
        let reply: Value = serde_json::from_str(&reply_raw).unwrap();
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["result"]["isError"], false);
        let rules: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_ne!(rules.as_array().unwrap().len(), 0);

        let modules_req = json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {"name": "offline_modules", "arguments": {}}
        })
        .to_string();
        let mod_raw = try_local_offline_tool(&modules_req).expect("offline_modules handled");
        let mod_reply: Value = serde_json::from_str(&mod_raw).unwrap();
        assert_eq!(mod_reply["result"]["isError"], false);

        let remote_req = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {"name": "project_list", "arguments": {}}
        })
        .to_string();
        assert!(try_local_offline_tool(&remote_req).is_none());
    }

    #[tokio::test]
    async fn prepare_request_preuploads_local_file_path_over_http() {
        use super::prepare_request;
        use crate::session::Remote;
        use rupicola_client::HttpApi;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let dir = tempfile::tempdir().unwrap();
        let shot_path = dir.path().join("shot.png");
        let payload_bytes = b"\x89PNG\r\n\x1a\nbridge-integration-bytes";
        std::fs::write(&shot_path, payload_bytes).unwrap();
        let expected_sha = sha256_hex(payload_bytes);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let sha_for_server = expected_sha.clone();
        let server = tokio::spawn(async move {
            // Request 1: POST /v1/projects/demo/blobs/missing
            let (mut s1, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = s1.read(&mut buf).await.unwrap();
            let body = json!({
                "missing": [sha_for_server],
                "limits": {
                    "max_blob_bytes": 1024,
                    "max_image_bytes": 1024,
                    "max_video_bytes": 1024,
                    "max_artifacts_per_capture": 10,
                    "max_body_bytes": 1024,
                    "max_missing_batch": 10,
                    "upload_timeout_secs": 60,
                    "upload_idle_secs": 30
                }
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            s1.write_all(resp.as_bytes()).await.unwrap();
            drop(s1);

            // Request 2: PUT /v1/projects/demo/blobs/{sha}
            let (mut s2, _) = listener.accept().await.unwrap();
            let mut buf2 = vec![0u8; 4096];
            let n = s2.read(&mut buf2).await.unwrap();
            let req_text = String::from_utf8_lossy(&buf2[..n]);
            assert!(
                req_text.starts_with("PUT /v1/projects/demo/blobs/"),
                "{req_text}"
            );
            s2.write_all(
                b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });

        let remote = Remote {
            api: HttpApi::new(&format!("http://127.0.0.1:{port}")).unwrap(),
            token: "test-token".to_owned(),
        };
        let incoming = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": {
                "name": "capture_publish",
                "arguments": {
                    "project": "demo",
                    "scenario": "bridge-int",
                    "members": [{"file_path": shot_path.to_str().unwrap()}]
                }
            }
        })
        .to_string();

        let rewritten = prepare_request(&remote, &incoming).await.unwrap();
        server.await.unwrap();

        let parsed: Value = serde_json::from_str(&rewritten).unwrap();
        let member = &parsed["params"]["arguments"]["members"][0];
        assert!(member.get("file_path").is_none());
        assert_eq!(member["sha256"], expected_sha);
        assert_eq!(member["bytes"], payload_bytes.len());
        assert_eq!(member["id"], "shot");
        assert_eq!(member["path"], "checkpoints/shot.png");
    }
}
