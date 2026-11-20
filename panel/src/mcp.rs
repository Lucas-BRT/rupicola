use super::{
    Arc, Body, PageResult, Panel, Response, StatusCode, auth, db_error, fix_record, header, ingest,
    pipeline, problem_record, safe_name, status,
};
use http::HeaderMap;
use serde_json::{Value as JsonValue, json};
use topcoat::router::to_bytes;

enum Scope {
    Project(String),
    Global { owner: String, is_owner: bool },
}

pub async fn handle(
    panel: Arc<Panel>,
    project: String,
    headers: &HeaderMap,
    body: Body,
) -> PageResult<Response> {
    if !safe_name(&project) {
        return Err(StatusCode::NOT_FOUND);
    }
    let digest = bearer_digest(&panel, headers)?;
    authorize_project(&panel, &project, &digest).await?;
    serve_jsonrpc(&panel, Scope::Project(project), body).await
}

pub async fn handle_global(
    panel: Arc<Panel>,
    headers: &HeaderMap,
    body: Body,
) -> PageResult<Response> {
    let digest = bearer_digest(&panel, headers)?;
    let (owner, is_owner) = authorize_global(&panel, &digest).await?;
    serve_jsonrpc(&panel, Scope::Global { owner, is_owner }, body).await
}

fn bearer_digest(panel: &Panel, headers: &HeaderMap) -> PageResult<String> {
    if headers.contains_key(header::ORIGIN) && !auth::same_origin(panel, headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    let secret = headers
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(auth::digest(secret))
}

async fn account_is_owner(db: &turso::Connection, handle: &str) -> PageResult<Option<bool>> {
    let mut rows = db
        .query("SELECT owner FROM accounts WHERE handle = ?", [handle])
        .await
        .map_err(db_error)?;
    rows.next()
        .await
        .map_err(db_error)?
        .map(|row| row.get::<i64>(0).map(|v| v == 1))
        .transpose()
        .map_err(db_error)
}

async fn is_member(db: &turso::Connection, project: &str, handle: &str) -> PageResult<bool> {
    let mut rows = db
        .query(
            "SELECT 1 FROM members WHERE project = ? AND handle = ?",
            [project, handle],
        )
        .await
        .map_err(db_error)?;
    Ok(rows.next().await.map_err(db_error)?.is_some())
}

async fn authorize_project(panel: &Panel, project: &str, digest: &str) -> PageResult<()> {
    let db = panel.index.lock().await;
    if ingest::device_label(&db, project, digest).await?.is_some() {
        db.execute(
            "UPDATE devices SET last_seen = ? WHERE digest = ?",
            turso::params![auth::now(), digest],
        )
        .await
        .map_err(db_error)?;
        return Ok(());
    }
    if let Some((owner, _)) = ingest::machine_token_owner(&db, digest).await? {
        let allowed = account_is_owner(&db, &owner).await?.unwrap_or(false)
            || is_member(&db, project, &owner).await?;
        if allowed {
            db.execute(
                "UPDATE machine_tokens SET last_seen = ? WHERE digest = ?",
                turso::params![auth::now(), digest],
            )
            .await
            .map_err(db_error)?;
            return Ok(());
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

async fn authorize_global(panel: &Panel, digest: &str) -> PageResult<(String, bool)> {
    let db = panel.index.lock().await;
    let Some((owner, _)) = ingest::machine_token_owner(&db, digest).await? else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let Some(is_owner) = account_is_owner(&db, &owner).await? else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    db.execute(
        "UPDATE machine_tokens SET last_seen = ? WHERE digest = ?",
        turso::params![auth::now(), digest],
    )
    .await
    .map_err(db_error)?;
    Ok((owner, is_owner))
}

async fn serve_jsonrpc(panel: &Panel, scope: Scope, body: Body) -> PageResult<Response> {
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
                (id.clone(), dispatch(panel, &scope, &request).await?)
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

#[expect(
    clippy::too_many_lines,
    reason = "the MCP dispatcher keeps method routing, scope checks, and responses together"
)]
async fn dispatch(panel: &Panel, scope: &Scope, request: &JsonValue) -> PageResult<JsonValue> {
    let params = &request["params"];
    Ok(match request["method"].as_str() {
        Some("initialize") => {
            let version = params["protocolVersion"]
                .as_str()
                .filter(|version| matches!(*version, "2025-03-26" | "2025-06-18" | "2025-11-25"))
                .unwrap_or("2025-11-25");
            json!({"result": {"protocolVersion": version, "capabilities": {"tools": {}},
                "serverInfo": {"name": "rupicola", "version": env!("CARGO_PKG_VERSION")}}})
        }
        Some("ping") => json!({"result": {}}),
        Some("tools/list") => {
            let mut tools = Vec::new();
            let is_global = matches!(scope, Scope::Global { .. });
            if is_global {
                tools.push(json!({
                    "name": "project_list",
                    "description": "List projects accessible to this token",
                    "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
                }));
            }
            for (name, description) in [
                (
                    "problem_list",
                    "List visual problems in the authorized project",
                ),
                ("fix_list", "List fixes in the authorized project"),
                (
                    "evidence_list",
                    "List the latest published captures in the authorized project",
                ),
            ] {
                let schema = if is_global {
                    json!({
                        "type": "object",
                        "properties": {"project": {"type": "string", "description": "Project ID"}},
                        "required": ["project"],
                        "additionalProperties": false
                    })
                } else {
                    json!({"type": "object", "properties": {}, "additionalProperties": false})
                };
                tools
                    .push(json!({"name": name, "description": description, "inputSchema": schema}));
            }
            json!({"result": {"tools": tools}})
        }
        Some("tools/call") => {
            let tool_name = params["name"].as_str();
            let args = &params["arguments"];
            if !args.is_null() && !args.is_object() {
                return Ok(error(-32602, "Arguments must be an object"));
            }
            if matches!(scope, Scope::Global { .. }) && tool_name == Some("project_list") {
                if !args.is_null() && args.as_object().is_some_and(|m| !m.is_empty()) {
                    return Ok(error(-32602, "project_list takes no arguments"));
                }
                let Scope::Global { owner, is_owner } = scope else {
                    unreachable!();
                };
                let db = panel.index.lock().await;
                let mut rows = if *is_owner {
                    db.query("SELECT id, label FROM projects ORDER BY label, id", ())
                        .await
                        .map_err(db_error)?
                } else {
                    db.query(
                        "SELECT p.id, p.label FROM projects p JOIN members m ON p.id = m.project
                         WHERE m.handle = ? ORDER BY p.label, p.id",
                        [owner.as_str()],
                    )
                    .await
                    .map_err(db_error)?
                };
                let mut projects = Vec::new();
                while let Some(row) = rows.next().await.map_err(db_error)? {
                    projects.push(json!({
                        "id": row.get::<String>(0).map_err(db_error)?,
                        "label": row.get::<String>(1).map_err(db_error)?,
                    }));
                }
                return Ok(
                    json!({"result": {"content": [{"type": "text", "text": json!(projects).to_string()}]}}),
                );
            }

            let target_project = match scope {
                Scope::Project(project) => {
                    if !args.is_null() && args.as_object().is_some_and(|m| !m.is_empty()) {
                        return Ok(error(-32602, "This tool takes no arguments"));
                    }
                    project.clone()
                }
                Scope::Global { owner, is_owner } => {
                    let Some(project) = args.get("project").and_then(|v| v.as_str()) else {
                        return Ok(error(-32602, "Missing required argument: project"));
                    };
                    if args.as_object().is_some_and(|m| m.len() != 1) || !safe_name(project) {
                        return Ok(error(-32602, "Invalid project argument"));
                    }
                    if !*is_owner {
                        let db = panel.index.lock().await;
                        if !is_member(&db, project, owner).await? {
                            return Ok(error(-32001, "Unauthorized for project"));
                        }
                    }
                    project.to_owned()
                }
            };

            let data = pipeline::data_root(panel);
            let output = match tool_name {
                Some("problem_list") => serde_json::to_value(
                    problem_record::entries(&data, &target_project)
                        .map_err(db_error)?
                        .into_iter()
                        .take(100)
                        .collect::<Vec<_>>(),
                ),
                Some("fix_list") => serde_json::to_value(
                    fix_record::entries(&data, &target_project)
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
                            [target_project.as_str()],
                        )
                        .await
                        .map_err(db_error)?;
                    let mut runs = Vec::new();
                    while let Some(row) = rows.next().await.map_err(db_error)? {
                        runs.push(json!({
                            "run": row.get::<String>(0).map_err(db_error)?,
                            "scenario": row.get::<String>(1).map_err(db_error)?,
                            "status": row.get::<String>(2).map_err(db_error)?,
                        }));
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
