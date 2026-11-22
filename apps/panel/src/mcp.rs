mod project_merge;
mod publish;

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
    serve_jsonrpc(&panel, Scope::Project(project), &digest, body).await
}

pub async fn handle_global(
    panel: Arc<Panel>,
    headers: &HeaderMap,
    body: Body,
) -> PageResult<Response> {
    let digest = bearer_digest(&panel, headers)?;
    let (owner, is_owner) = authorize_global(&panel, &digest).await?;
    serve_jsonrpc(&panel, Scope::Global { owner, is_owner }, &digest, body).await
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

const PROJECT_READS: &[(&str, &str, Option<&str>)] = &[
    (
        "problem_list",
        "List visual problems in the authorized project",
        None,
    ),
    (
        "fix_list",
        "List recorded fixes in the authorized project",
        None,
    ),
    (
        "evidence_list",
        "List indexed captures in the authorized project",
        None,
    ),
    (
        "evidence_get",
        "Read the manifest of an authorized capture",
        Some("run"),
    ),
    (
        "design_list",
        "Read design briefs, directions, tactics and decisions",
        None,
    ),
    (
        "propose_list",
        "List proposal summaries in the authorized project",
        None,
    ),
    (
        "propose_get",
        "Read a proposal including its comments and attachments",
        Some("id"),
    ),
    (
        "design_system_list",
        "List design system versions and identify the adopted version",
        None,
    ),
    (
        "design_system_get",
        "Read a design system document, review history and validation results",
        Some("id"),
    ),
    (
        "problem_get",
        "Read a problem and its recorded evidence in the authorized project",
        Some("id"),
    ),
    (
        "fix_get",
        "Read a fix and its comparison pairs in the authorized project",
        Some("id"),
    ),
    (
        "artifact_read",
        "Read up to 256 KiB of a declared project artifact by its /media/ URL",
        Some("url"),
    ),
    (
        "design_system_preview",
        "Read rendered HTML for a Design System version",
        Some("version"),
    ),
    (
        "design_system_proof",
        "Read a validation screenshot by version/name",
        Some("proof"),
    ),
    (
        "propose_asset_read",
        "Read a proposal attachment or preview by proposal_id/filename",
        Some("asset"),
    ),
    (
        "propose_dispatch_list",
        "List pending project proposal dispatches without consuming them",
        None,
    ),
    (
        "artifact_read_chunk",
        "Read a 256 KiB chunk of a declared artifact. Pass offset:/media/root/run/path.",
        Some("chunk"),
    ),
    (
        "storage_status",
        "Read storage totals and usage by run and Fix for the authorized project",
        None,
    ),
];

fn read_artifact(
    panel: &Panel,
    project: &str,
    input: &str,
    chunked: bool,
) -> PageResult<JsonValue> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::io::{Read as _, Seek as _, SeekFrom};
    const LIMIT: u64 = 256 * 1024;
    let (offset, url) = if chunked {
        let (offset, url) = input.split_once(':').ok_or(StatusCode::BAD_REQUEST)?;
        (
            offset.parse::<u64>().map_err(|_| StatusCode::BAD_REQUEST)?,
            url,
        )
    } else {
        (0, input)
    };
    let (path, _, _) = publish::evidence_location(panel, project, url)?;
    let mut file = std::fs::File::open(path).map_err(db_error)?;
    let size = file.metadata().map_err(db_error)?.len();
    if offset > size {
        return Err(StatusCode::BAD_REQUEST);
    }
    file.seek(SeekFrom::Start(offset)).map_err(db_error)?;
    let mut bytes = Vec::new();
    file.take(if chunked { LIMIT } else { LIMIT + 1 })
        .read_to_end(&mut bytes)
        .map_err(db_error)?;
    if bytes.len() as u64 > LIMIT {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let next = offset + bytes.len() as u64;
    Ok(
        json!({"url":url,"encoding":"base64","data":STANDARD.encode(&bytes),"size":size,
        "offset":offset,"next_offset":(next < size && !bytes.is_empty()).then_some(next)}),
    )
}

async fn project_read(
    panel: &Panel,
    project: &str,
    tool: &str,
    argument: Option<&str>,
) -> PageResult<Option<JsonValue>> {
    let data = pipeline::data_root(panel);
    let proposals = data.join(crate::proposal_record::DIRECTORY).join(project);
    let result = match tool {
        "artifact_read" | "artifact_read_chunk" => read_artifact(
            panel, project, argument.unwrap_or_default(), tool == "artifact_read_chunk",
        )?,
        "design_system_preview" => json!({"mime":"text/html","html":
            crate::design_system::preview_html(panel, project, argument.unwrap_or_default()).await?}),
        "design_system_proof" | "propose_asset_read" => {
            use base64::{Engine as _, engine::general_purpose::STANDARD};
            let (id, name) = argument.and_then(|value| value.split_once('/'))
                .ok_or(StatusCode::BAD_REQUEST)?;
            let (mime, data) = if tool == "design_system_proof" {
                ("image/png", crate::design_system::proof_base64(panel, project, id, name).await?)
            } else {
                let (mime, bytes) = crate::proposes::read_asset(panel, project, id, name)?;
                (mime, STANDARD.encode(bytes))
            };
            if data.len() > 4 * 1024 * 1024 {
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }
            json!({"mime":mime,"encoding":"base64","data":data})
        }
        "propose_dispatch_list" => serde_json::to_value(
            crate::proposal_record::dispatches(&proposals).map_err(db_error)?.into_iter()
                .filter(|item| item.project_id == project).take(25).collect::<Vec<_>>()
        ).map_err(db_error)?,
        "storage_status" => {
            let report = crate::storage::analyze(&data, Some(project));
            json!({"total_bytes":report.total_bytes,"total_files":report.total_files,
                "pinned_bytes":report.pinned_bytes,"reclaimable_bytes":report.reclaimable_bytes,
                "raw_frames_bytes":report.raw_frames_bytes,"offloaded_runs":report.offloaded_runs,
                "runs":report.by_run.iter().take(100).map(|run| json!({"name":run.run_name,
                    "bytes":run.total_bytes,"raw_frames_bytes":run.raw_frames_bytes,"offloaded":run.offloaded,
                    "pinned_by_fixes":run.pinned_by_fixes,"pinned_by_problems":run.pinned_by_problems})).collect::<Vec<_>>(),
                "fixes":report.by_fix.into_iter().take(100).collect::<Vec<_>>()})
        }
        "problem_get" => {
            let problem = problem_record::load(&data, project, argument.unwrap_or_default())
                .map_err(|_| StatusCode::NOT_FOUND)?;
            let skill_modules = problem_record::recommended_skill_modules(&problem);
            let mut value = serde_json::to_value(&problem).map_err(db_error)?;
            if let Some(obj) = value.as_object_mut() {
                obj.insert("skill_modules".into(), json!(skill_modules));
            }
            value
        }
        "fix_get" => serde_json::to_value(
            fix_record::load(&data, project, argument.unwrap_or_default())
                .map_err(|_| StatusCode::NOT_FOUND)?,
        ).map_err(db_error)?,
        "evidence_get" => {
            let manifest = crate::read_run(panel, argument.unwrap_or_default())
                .await
                .filter(|manifest| manifest.project.id == project)
                .ok_or(StatusCode::NOT_FOUND)?;
            json!({"project": manifest.project.id, "scenario": manifest.scenario,
                "revision": manifest.revision, "status": manifest.status,
                "inspection": manifest.inspection, "created_at_ms": manifest.created_at_ms,
                "checkpoints": manifest.checkpoints.iter().map(|item| json!({"id": item.id, "path": item.path})).collect::<Vec<_>>(),
                "clips": manifest.clips.iter().map(|item| json!({"id": item.id, "video_path": item.video_path,
                    "sheet_path": item.sheet_path, "analysis_path": item.analysis_path,
                    "frames": item.frames.iter().map(|frame| json!({"path": frame.path, "target_ms": frame.target_ms, "video_ms": frame.video_ms})).collect::<Vec<_>>() })).collect::<Vec<_>>()})
        }
        "design_list" => {
            let root = panel.design.join(project);
            let entries = match crate::design_record::entries(&root) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                result => result.map_err(db_error)?,
            };
            serde_json::to_value(entries.into_iter().take(100).collect::<Vec<_>>()).map_err(db_error)?
        }
        "propose_list" => json!(crate::proposal_record::entries(&proposals).map_err(db_error)?.into_iter().take(100)
            .map(|p| json!({"id": p.id, "title": p.title, "kind": p.kind, "status": p.status, "parent_id": p.parent_id})).collect::<Vec<_>>()),
        "propose_get" => {
            let id = argument.filter(|id| crate::proposal_record::safe_id(id)).ok_or(StatusCode::BAD_REQUEST)?;
            let proposal = crate::proposal_record::read_proposal(&proposals.join(format!("{id}.json")))
                .map_err(|_| StatusCode::NOT_FOUND)?;
            serde_json::to_value(proposal).map_err(db_error)?
        }
        "design_system_list" | "design_system_get" => crate::design_system::mcp_read(panel, project, argument).await?,
        _ => return Ok(None),
    };
    Ok(Some(result))
}

fn payload(output: &JsonValue) -> JsonValue {
    json!({"result": {"content": [{"type": "text", "text": output.to_string()}]}})
}

fn writer<'a>(panel: &Panel, scope: &'a Scope) -> Option<&'a str> {
    if !panel.mcp_write {
        return None;
    }
    match scope {
        Scope::Global {
            owner,
            is_owner: true,
        } => Some(owner),
        _ => None,
    }
}

async fn serve_jsonrpc(
    panel: &Panel,
    scope: Scope,
    digest: &str,
    body: Body,
) -> PageResult<Response> {
    let permit = if writer(panel, &scope).is_some() && panel.uploads.is_some() {
        Some(
            panel
                .upload_work
                .clone()
                .try_acquire_owned()
                .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?,
        )
    } else {
        None
    };
    let (limit, timeout) = if permit.is_some() {
        (crate::ingest::MAX_ENVELOPE, 90)
    } else if writer(panel, &scope).is_some() {
        (1024 * 1024, 5)
    } else {
        (16 * 1024, 5)
    };
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(timeout),
        to_bytes(body, limit),
    )
    .await
    .map_err(|_| StatusCode::REQUEST_TIMEOUT)?
    .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    let large = bytes.len() > 1024 * 1024;
    let parsed = serde_json::from_slice::<JsonValue>(&bytes);
    drop(bytes);
    let capture = parsed.as_ref().is_ok_and(|request| {
        request["method"] == "tools/call" && request["params"]["name"] == "capture_publish"
    });
    if large && !capture {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let permit = if capture { permit } else { None };
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
                (
                    id.clone(),
                    dispatch(panel, &scope, digest, &request, permit).await?,
                )
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
async fn dispatch(
    panel: &Panel,
    scope: &Scope,
    digest: &str,
    request: &JsonValue,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
) -> PageResult<JsonValue> {
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
            for &(name, description, argument) in PROJECT_READS {
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();
                if is_global {
                    properties.insert(
                        "project".into(),
                        json!({"type": "string", "description": "Project ID"}),
                    );
                    required.push("project");
                }
                if let Some(key) = argument {
                    properties.insert(key.into(), json!({"type": "string"}));
                    required.push(key);
                }
                tools.push(json!({"name": name, "description": description, "inputSchema":
                    {"type": "object", "properties": properties, "required": required, "additionalProperties": false}}));
            }
            if writer(panel, scope).is_some() {
                tools.push(json!({
                    "name":"capture_publish",
                    "description":"Publish a capture bundle. JSON-RPC requests are limited to 64 MiB including the envelope.",
                    "inputSchema":{"type":"object","required":["project","bundle"],"additionalProperties":false,
                    "properties":{"project":{"type":"string"},"bundle":{"type":"object"}}}
                }));
                tools.extend(publish::tools());
                tools.extend(publish::update_tools());
                tools.extend(publish::design_tools());
                tools.push(json!({"name":"project_merge_apply",
                    "description":"Merge registered projects with captures, proposal assets and queues, and database records. Blocks incompatible access unless revoke_source_access is explicitly true.",
                    "inputSchema":{"type":"object","required":["source","target"],"additionalProperties":false,
                    "properties":{"source":{"type":"string"},"target":{"type":"string"},"revoke_source_access":{"type":"boolean","default":false}}}}));
                tools.push(json!({"name":"project_merge_preview",
                    "description":"Inventory a proposed project merge and report overlapping records and access entries",
                    "inputSchema":{"type":"object","required":["source","target"],"additionalProperties":false,
                    "properties":{"source":{"type":"string"},"target":{"type":"string"}}}}));
                tools.push(json!({"name":"storage_config",
                    "description":"Read panel-wide bucket settings, or replace them by providing config. Set mounted filesystem paths in the panel.",
                    "inputSchema":{"type":"object","additionalProperties":false,"properties":{"config":{"type":"object",
                    "required":["provider","bucket","endpoint_url","region","prefix","public_base_url","auto_policy","delete_local_after_offload","keep_latest_runs"],
                    "properties":{"provider":{"enum":["s3-compatible","rclone","filesystem-bucket"]},"bucket":{"type":"string"},
                        "endpoint_url":{"type":"string"},"region":{"type":"string"},"prefix":{"type":"string"},"public_base_url":{"type":"string"},
                        "auto_policy":{"enum":["manual","on-fix-approved","after-capture"]},"delete_local_after_offload":{"type":"boolean"},
                        "keep_latest_runs":{"type":"integer","minimum":0,"maximum":100}}}}}}));
                tools.push(json!({"name":"project_rename",
                    "description":"Change a project's display label while preserving its stable ID",
                    "inputSchema":{"type":"object","required":["project","label"],"additionalProperties":false,
                    "properties":{"project":{"type":"string"},"label":{"type":"string"}}}}));
                tools.push(json!({"name":"project_access",
                    "description":"List project members, devices and audit events, or manage project access. Creation returns a secret once.",
                    "inputSchema":{"type":"object","required":["project","action"],"additionalProperties":false,
                    "properties":{"project":{"type":"string"},"action":{"enum":["list","invite","device","revoke-member","revoke-device"]},
                                 "value":{"type":"string","description":"Role, device label, member handle or device ID for the selected action"}}}}));
                tools.push(json!({"name":"storage_run_action",
                    "description":"Offload one capture to the configured bucket or delete one unpinned capture",
                    "inputSchema":{"type":"object","required":["project","run","action"],"additionalProperties":false,
                    "properties":{"project":{"type":"string"},"run":{"type":"string"},"action":{"enum":["offload","delete"]}}}}));
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

            if matches!(tool_name, Some("project_access" | "project_rename")) {
                let Some(actor) = writer(panel, scope) else {
                    return Ok(error(-32001, "An enabled owner machine token is required"));
                };
                let result = if tool_name == Some("project_access") {
                    publish::project_access(panel, actor, args).await
                } else {
                    publish::project_rename(panel, actor, args).await
                };
                return match result {
                    Ok(result) => Ok(payload(&result)),
                    Err(StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY) => {
                        Ok(error(-32602, "Invalid project administration arguments"))
                    }
                    Err(StatusCode::NOT_FOUND) => Ok(error(-32004, "Project not found")),
                    Err(status) => Err(status),
                };
            }
            if matches!(
                tool_name,
                Some("project_merge_preview" | "project_merge_apply")
            ) {
                let Some(actor) = writer(panel, scope) else {
                    return Ok(error(-32001, "An enabled owner machine token is required"));
                };
                let result = if tool_name == Some("project_merge_apply") {
                    project_merge::apply(panel, actor, args).await
                } else {
                    project_merge::preview(panel, args).await
                };
                return match result {
                    Ok(preview) => Ok(payload(&preview)),
                    Err(StatusCode::BAD_REQUEST) => {
                        Ok(error(-32602, "Invalid source or target project"))
                    }
                    Err(StatusCode::NOT_FOUND) => {
                        Ok(error(-32004, "Source or target project not found"))
                    }
                    Err(StatusCode::CONFLICT) => Ok(error(
                        -32009,
                        "Merge blocked by files, access differences, another merge or conflicting records",
                    )),
                    Err(status) => Err(status),
                };
            }
            if tool_name == Some("storage_config") {
                if writer(panel, scope).is_none() {
                    return Ok(error(-32001, "An enabled owner machine token is required"));
                }
                let input = if args.is_null() {
                    json!({})
                } else {
                    args.clone()
                };
                return match publish::storage_config(panel, &input) {
                    Ok(config) => Ok(payload(&config)),
                    Err(StatusCode::BAD_REQUEST) => {
                        Ok(error(-32602, "Invalid bucket configuration"))
                    }
                    Err(status) => Err(status),
                };
            }
            if tool_name == Some("storage_run_action") {
                if writer(panel, scope).is_none() {
                    return Ok(error(-32001, "An enabled owner machine token is required"));
                }
                return match publish::storage_run_action(panel, args).await {
                    Ok(summary) => Ok(payload(&summary)),
                    Err(StatusCode::BAD_REQUEST) => Ok(error(-32602, "Invalid storage arguments")),
                    Err(StatusCode::NOT_FOUND) => {
                        Ok(error(-32004, "Capture not found in project storage"))
                    }
                    Err(StatusCode::CONFLICT) => Ok(error(-32009, "Capture is pinned")),
                    Err(status) => Err(status),
                };
            }

            if tool_name == Some("capture_publish") {
                if writer(panel, scope).is_none() {
                    return Ok(error(-32001, "An enabled owner machine token is required"));
                }
                let permit = permit.ok_or(StatusCode::NOT_FOUND)?;
                return match publish::capture(panel, digest, args, permit).await {
                    Ok(bundle) => Ok(payload(&bundle)),
                    Err(StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY) => {
                        Ok(error(-32602, "Invalid capture bundle"))
                    }
                    Err(status) => Err(status),
                };
            }

            if let Some(name) = tool_name
                && (publish::CREATES.contains(&name)
                    || publish::UPDATES.iter().any(|(tool, _, _, _)| *tool == name)
                    || publish::DESIGN_TOOLS.contains(&name))
            {
                let Some(actor) = writer(panel, scope) else {
                    return Ok(error(-32001, "An enabled owner machine token is required"));
                };
                let result = if publish::CREATES.contains(&name) {
                    publish::create(panel, actor, name, args).await
                } else if publish::DESIGN_TOOLS.contains(&name) {
                    publish::design(panel, actor, name, args).await
                } else {
                    publish::update(panel, actor, name, args).await
                };
                return match result {
                    Ok(result) => Ok(payload(&result)),
                    Err(StatusCode::NOT_FOUND) => Ok(error(
                        -32004,
                        "Project, evidence or linked record not found",
                    )),
                    Err(StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY) => {
                        Ok(error(-32602, "Invalid record arguments"))
                    }
                    Err(StatusCode::CONFLICT) => {
                        Ok(error(-32009, "Record state or generation conflict"))
                    }
                    Err(status) => Err(status),
                };
            }
            let Some(&(_, _, parameter)) = PROJECT_READS
                .iter()
                .find(|(name, _, _)| Some(*name) == tool_name)
            else {
                return Ok(error(-32602, "Unknown tool"));
            };
            let argument = parameter
                .and_then(|name| args.get(name))
                .and_then(JsonValue::as_str);
            if (parameter.is_some()
                && argument.is_none_or(|value| value.is_empty() || value.len() > 256))
                || args.as_object().is_some_and(|map| {
                    map.len()
                        != usize::from(parameter.is_some())
                            + usize::from(matches!(scope, Scope::Global { .. }))
                })
            {
                return Ok(error(-32602, "Invalid tool arguments"));
            }
            let target_project = match scope {
                Scope::Project(project) => project.as_str(),
                Scope::Global { owner, is_owner } => {
                    let Some(project) = args
                        .get("project")
                        .and_then(JsonValue::as_str)
                        .filter(|p| safe_name(p))
                    else {
                        return Ok(error(-32602, "Missing or invalid project argument"));
                    };
                    if !*is_owner {
                        let db = panel.index.lock().await;
                        if !is_member(&db, project, owner).await? {
                            return Ok(error(-32001, "Unauthorized for project"));
                        }
                    }
                    project
                }
            };
            match project_read(
                panel,
                target_project,
                tool_name.unwrap_or_default(),
                argument,
            )
            .await
            {
                Ok(Some(result)) => return Ok(payload(&result)),
                Ok(None) => (),
                Err(StatusCode::NOT_FOUND) => {
                    return Ok(error(-32004, "Resource not found in project"));
                }
                Err(StatusCode::BAD_REQUEST) => {
                    return Ok(error(-32602, "Invalid resource identifier"));
                }
                Err(StatusCode::PAYLOAD_TOO_LARGE) => {
                    return Ok(error(-32013, "Artifact exceeds the MCP size limit"));
                }
                Err(status) => return Err(status),
            }

            let data = pipeline::data_root(panel);
            let output = match tool_name {
                Some("problem_list") => serde_json::to_value(
                    problem_record::entries(&data, target_project)
                        .map_err(db_error)?
                        .into_iter()
                        .take(100)
                        .collect::<Vec<_>>(),
                ),
                Some("fix_list") => serde_json::to_value(
                    fix_record::entries(&data, target_project)
                        .map_err(db_error)?
                        .into_iter()
                        .take(100)
                        .collect::<Vec<_>>(),
                ),
                Some("evidence_list") => {
                    let runs = crate::read_index(panel).await.map_err(db_error)?;
                    Ok(json!(
                        runs.iter()
                            .filter(|run| run.project_id == target_project)
                            .take(100)
                            .map(|run| json!({
                                "run": run.key,
                                "scenario": run.scenario,
                                "status": run.status,
                            }))
                            .collect::<Vec<_>>()
                    ))
                }
                _ => return Ok(error(-32602, "Unknown tool")),
            }
            .map_err(db_error)?;
            json!({"result": {"content": [{"type": "text", "text": output.to_string()}]}})
        }
        _ => error(-32601, "Method not found"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scoped_and_non_owner_tokens_cannot_publish_problems() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("panel.db");
        let database = turso::Builder::new_local(path.to_str().unwrap())
            .build()
            .await
            .unwrap();
        let index = database.connect().unwrap();
        auth::init(&index).await.unwrap();
        index
            .execute(
                "INSERT INTO projects (id, label) VALUES ('sample','Sample')",
                (),
            )
            .await
            .unwrap();
        let mut panel = Panel {
            design: temp.path().join("design"),
            roots: vec![],
            index: tokio::sync::Mutex::new(index),
            origin: String::new(),
            localhost_origin: String::new(),
            host: String::new(),
            secure: false,
            bootstrap: None,
            dummy_password: String::new(),
            password_work: Arc::new(tokio::sync::Semaphore::new(1)),
            upload_work: Arc::new(tokio::sync::Semaphore::new(1)),
            design_validation: tokio::sync::Semaphore::new(1),
            login_attempts: tokio::sync::Mutex::default(),
            uploads: None,
            mcp_write: true,
            blobs: crate::blobs::Blobs::open(temp.path().join("blobs")).unwrap(),
            root_keys: vec![],
        };
        let request = json!({"method":"tools/call","params":{"name":"problem_create",
            "arguments":{"project":"sample","key":"problem-1","record":{
            "title":"Overflow","diagnosis":"Long title","solution":"Wrap","severity":"major"}}}});
        let owner = Scope::Global {
            owner: "owner".into(),
            is_owner: true,
        };
        for scope in [
            Scope::Project("sample".into()),
            Scope::Global {
                owner: "member".into(),
                is_owner: false,
            },
        ] {
            assert_eq!(
                dispatch(&panel, &scope, "", &request, None).await.unwrap()["error"]["code"],
                -32001
            );
        }
        panel.mcp_write = false;
        assert_eq!(
            dispatch(&panel, &owner, "", &request, None).await.unwrap()["error"]["code"],
            -32001
        );
        panel.mcp_write = true;
        for expected in [true, false] {
            let reply = dispatch(&panel, &owner, "", &request, None).await.unwrap();
            let record: JsonValue =
                serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(record["created"], expected);
        }

        let rename = json!({"method":"tools/call","params":{"name":"project_rename",
            "arguments":{"project":"sample","label":"Rupicola"}}});
        assert!(
            dispatch(&panel, &owner, "", &rename, None)
                .await
                .unwrap()
                .get("result")
                .is_some()
        );
        let access = json!({"method":"tools/call","params":{"name":"project_access",
            "arguments":{"project":"sample","action":"list"}}});
        let reply = dispatch(&panel, &owner, "", &access, None).await.unwrap();
        let details: JsonValue =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(details["audit"][0]["event"], "Project label changed");
        let project_scope = Scope::Project("sample".into());
        assert_eq!(
            dispatch(&panel, &project_scope, "", &access, None)
                .await
                .unwrap()["error"]["code"],
            -32001
        );
    }
}
