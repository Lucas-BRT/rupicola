use super::*;
use http::HeaderMap;
use serde_json::{Value as JsonValue, json};
use topcoat::router::to_bytes;

pub async fn handle(
    panel: Arc<Panel>,
    project: String,
    headers: &HeaderMap,
    body: Body,
) -> PageResult<Response> {
    if !safe_name(&project) {
        return Err(StatusCode::NOT_FOUND);
    }
    if headers.contains_key(header::ORIGIN) && !auth::same_origin(&panel, headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    let secret = headers
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let digest = auth::digest(secret);
    {
        let db = panel.index.lock().await;
        if ingest::device_label(&db, &project, &digest)
            .await?
            .is_none()
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    let bytes = tokio::time::timeout(std::time::Duration::from_secs(5), to_bytes(body, 16 * 1024))
        .await
        .map_err(|_| StatusCode::REQUEST_TIMEOUT)?
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    let parsed = serde_json::from_slice::<JsonValue>(&bytes);
    let (id, payload) = match parsed {
        Err(_) => (JsonValue::Null, error(-32700, "Parse error")),
        Ok(request) if request["jsonrpc"] != "2.0" || !request["method"].is_string() => {
            (JsonValue::Null, error(-32600, "Invalid request"))
        }
        Ok(request) => {
            let Some(id) = request.get("id") else {
                return Ok(status(StatusCode::ACCEPTED));
            };
            if !id.is_number() && !id.is_string() {
                (JsonValue::Null, error(-32600, "Invalid request"))
            } else {
                (id.clone(), dispatch(&panel, &project, &request).await?)
            }
        }
    };
    let mut reply = payload;
    reply["jsonrpc"] = json!("2.0");
    reply["id"] = id;
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(reply.to_string()))
        .map_err(db_error)
}

fn error(code: i32, message: &str) -> JsonValue {
    json!({"error": {"code": code, "message": message}})
}

async fn dispatch(panel: &Panel, project: &str, request: &JsonValue) -> PageResult<JsonValue> {
    let params = &request["params"];
    Ok(match request["method"].as_str() {
        Some("initialize") => {
            let version = match params["protocolVersion"].as_str() {
                Some(version @ ("2025-03-26" | "2025-06-18" | "2025-11-25")) => version,
                _ => "2025-11-25",
            };
            json!({"result": {"protocolVersion": version, "capabilities": {"tools": {}},
                "serverInfo": {"name": "rupicola", "version": env!("CARGO_PKG_VERSION")}}})
        }
        Some("ping") => json!({"result": {}}),
        Some("tools/list") => {
            let tools: Vec<_> = [
                ("problem_list", "List visual problems in the authorized project"),
                ("fix_list", "List fixes in the authorized project"),
                ("evidence_list", "List the latest published captures in the authorized project"),
            ]
            .into_iter()
            .map(|(name, description)| {
                json!({"name": name, "description": description,
                    "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}})
            })
            .collect();
            json!({"result": {"tools": tools}})
        }
        Some("tools/call") => {
            if !params["arguments"].is_null()
                && !params["arguments"]
                    .as_object()
                    .is_some_and(|args| args.is_empty())
            {
                return Ok(error(-32602, "This tool takes no arguments"));
            }
            let data = pipeline::data_root(panel);
            let output = match params["name"].as_str() {
                Some("problem_list") => serde_json::to_value(
                    problem_record::entries(&data, project)
                        .map_err(db_error)?
                        .into_iter()
                        .take(100)
                        .collect::<Vec<_>>(),
                ),
                Some("fix_list") => serde_json::to_value(
                    fix_record::entries(&data, project)
                        .map_err(db_error)?
                        .into_iter()
                        .take(100)
                        .collect::<Vec<_>>(),
                ),
                Some("evidence_list") => {
                    let db = panel.index.lock().await;
                    let mut rows = db
                        .query(
                            "SELECT key, scenario, status FROM runs_v1 WHERE project_id = ?
                         ORDER BY created_at_ms DESC, key LIMIT 100",
                            [project],
                        )
                        .await
                        .map_err(db_error)?;
                    let mut runs = Vec::new();
                    while let Some(row) = rows.next().await.map_err(db_error)? {
                        runs.push(json!({"run": row.get::<String>(0).map_err(db_error)?,
                            "scenario": row.get::<String>(1).map_err(db_error)?,
                            "status": row.get::<String>(2).map_err(db_error)?}));
                    }
                    Ok(json!(runs))
                }
                _ => return Ok(error(-32602, "Unknown tool")),
            }
            .map_err(db_error)?;
            json!({"result": {"content": [{"type": "text", "text": output.to_string()}]}})
        }
        _ => error(-32601, "Method not found"),
    })
}
