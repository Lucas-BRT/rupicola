//! The Model Context Protocol endpoint (`POST /mcp`): JSON-RPC 2.0 whose tools call the
//! same services as the REST API, so an agent has exactly the access its token gives.
//!
//! Tools are the variants of [`Call`]; [`catalog`] describes them. A test keeps the two
//! in step.

use crate::{
    domain::{ApiError, Principal},
    services::{capture_runner, captures, design, fixes, problems, projects},
    state::Shared,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::Engine as _;
use rupicola_api::{
    NewFinding, NewFix, NewProblem, NewProject, NewReview, NewStep, ProblemPatch, RunCapture,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Protocol revisions the server speaks, newest last.
const PROTOCOL_VERSIONS: &[&str] = &["2025-03-26", "2025-06-18", "2025-11-25"];
/// Revision answered when the client asks for one the server does not know.
const DEFAULT_PROTOCOL_VERSION: &str = "2025-11-25";
const SERVER_NAME: &str = "rupicola";
/// JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
/// Bytes of an artifact one `artifact_read` returns unless asked for fewer.
const DEFAULT_READ_BYTES: usize = 256 * 1024;
/// Most bytes one `artifact_read` may return.
const MAX_READ_BYTES: usize = 1024 * 1024;

fn default_read_bytes() -> usize {
    DEFAULT_READ_BYTES
}

/// One tool invocation: the tool's name and its arguments.
#[derive(Deserialize)]
#[serde(tag = "name", content = "arguments", rename_all = "snake_case")]
enum Call {
    ProjectList {},
    ProjectCreate(NewProject),
    CaptureList {
        project: String,
    },
    CaptureGet {
        id: String,
    },
    CaptureRun {
        project: String,
        #[serde(flatten)]
        capture: RunCapture,
    },
    CaptureCapabilities {},
    CaptureReview {
        id: String,
        #[serde(flatten)]
        finding: NewFinding,
    },
    ArtifactRead {
        id: String,
        #[serde(default)]
        offset: u64,
        #[serde(default = "default_read_bytes")]
        length: usize,
    },
    ProblemList {
        project: String,
    },
    ProblemGet {
        id: String,
    },
    ProblemCreate {
        project: String,
        #[serde(flatten)]
        problem: NewProblem,
    },
    ProblemTriage {
        id: String,
        #[serde(flatten)]
        patch: ProblemPatch,
    },
    ProblemComment {
        id: String,
        #[serde(flatten)]
        comment: rupicola_api::NewComment,
    },
    ProblemAnalyze {
        project: String,
        capture: String,
    },
    DesignList {
        project: String,
    },
    DesignRecord {
        project: String,
        #[serde(flatten)]
        entry: rupicola_api::NewDesignEntry,
    },
    FixList {
        project: String,
    },
    FixGet {
        id: String,
    },
    FixCreate {
        project: String,
        #[serde(flatten)]
        fix: NewFix,
    },
    FixStep {
        id: String,
        #[serde(flatten)]
        step: NewStep,
    },
    FixReview {
        id: String,
        #[serde(flatten)]
        review: NewReview,
    },
}

/// A handler's JSON body, as a value.
fn value<T: Serialize>(body: &T) -> Result<Value, ApiError> {
    Ok(serde_json::to_value(body).map_err(anyhow::Error::from)?)
}

impl Call {
    async fn run(self, state: Shared, principal: Principal) -> Result<Value, ApiError> {
        match self {
            Self::ProjectList {} => value(&projects::list(&state, &principal).await?),
            Self::ProjectCreate(form) => value(&projects::create(&state, &principal, form).await?),
            Self::CaptureList { project } => {
                value(&captures::list(&state, &principal, &project).await?)
            }
            Self::CaptureGet { id } => value(&captures::get(&state, &principal, &id).await?),
            Self::CaptureRun { project, capture } => {
                value(&capture_runner::run(&state, &principal, &project, capture).await?)
            }
            Self::CaptureCapabilities {} => value(&capture_runner::capabilities().await),
            Self::CaptureReview { id, finding } => {
                value(&captures::add_finding(&state, &principal, &id, finding).await?)
            }
            Self::ArtifactRead { id, offset, length } => {
                read_artifact(&state, &principal, &id, offset, length).await
            }
            Self::ProblemList { project } => {
                value(&problems::list(&state, &principal, &project).await?)
            }
            Self::ProblemGet { id } => value(&problems::get(&state, &principal, &id).await?),
            Self::ProblemCreate { project, problem } => {
                value(&problems::create(&state, &principal, &project, &problem).await?)
            }
            Self::ProblemTriage { id, patch } => {
                value(&problems::update(&state, &principal, &id, &patch).await?)
            }
            Self::ProblemComment { id, comment } => {
                value(&problems::comment(&state, &principal, &id, &comment).await?)
            }
            Self::ProblemAnalyze { project, capture } => {
                value(&problems::analyze_capture(&state, &principal, &project, &capture).await?)
            }
            Self::DesignList { project } => {
                value(&design::list(&state, &principal, &project).await?)
            }
            Self::DesignRecord { project, entry } => {
                value(&design::create(&state, &principal, &project, entry).await?)
            }
            Self::FixList { project } => value(&fixes::list(&state, &principal, &project).await?),
            Self::FixGet { id } => value(&fixes::get(&state, &principal, &id).await?),
            Self::FixCreate { project, fix } => {
                value(&fixes::create(&state, &principal, &project, &fix).await?)
            }
            Self::FixStep { id, step } => {
                value(&fixes::add_step(&state, &principal, &id, step).await?)
            }
            Self::FixReview { id, review } => {
                value(&fixes::review(&state, &principal, &id, &review).await?)
            }
        }
    }
}

/// A slice of an artifact as base64, with the media type and total size, so an agent can
/// page through files it cannot fetch over HTTP.
async fn read_artifact(
    state: &Shared,
    principal: &Principal,
    id: &str,
    offset: u64,
    length: usize,
) -> Result<Value, ApiError> {
    let (blob, data) =
        captures::read_artifact_range(state, principal, id, offset, length.min(MAX_READ_BYTES))
            .await?;
    Ok(json!({
        "mime": blob.mime,
        "bytes": blob.bytes,
        "offset": offset,
        "length": data.len(),
        "data": base64::engine::general_purpose::STANDARD.encode(data),
    }))
}

/// One tool's catalog entry.
struct Tool {
    name: &'static str,
    description: &'static str,
    required: &'static [&'static str],
    properties: fn() -> Value,
}

impl Tool {
    fn describe(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": {
                "type": "object",
                "required": self.required,
                "properties": (self.properties)(),
            },
        })
    }
}

fn text() -> Value {
    json!({"type": "string"})
}

fn string_list() -> Value {
    json!({"type": "array", "items": {"type": "string"}})
}

fn severity() -> Value {
    json!({"enum": ["critical", "major", "minor", "polish"]})
}

fn comparisons() -> Value {
    json!({"type": "array", "items": {"type": "object",
        "required": ["label", "before", "after"],
        "properties": {"label": text(), "route": text(), "viewport": text(), "theme": text(),
            "before": text(), "after": text(), "diff": text(),
            "changed_ratio": {"type": "number"}}}})
}

fn step_properties() -> Value {
    json!({"title": text(), "detail": text(), "verification": text(), "evidence": text()})
}

/// Every tool, in the order clients list them.
const TOOLS: &[Tool] = &[
    Tool {
        name: "project_list",
        description: "List the projects this credential can see",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "project_create",
        description: "Create a project with a stable slug id and a label",
        required: &["id", "label"],
        properties: || json!({"id": text(), "label": text()}),
    },
    Tool {
        name: "capture_list",
        description: "List a project's captures, newest first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "capture_get",
        description: "One capture with its checkpoints, clips and review notes",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "capture_run",
        description: "Execute a headless web capture on the server and store its checkpoints and clips",
        required: &["project", "scenario", "url"],
        properties: || {
            let action = json!({
                "type": "object",
                "required": ["kind", "selector"],
                "properties": {
                    "kind": {"enum": ["click", "hover", "fill", "press", "ready"]},
                    "selector": text(),
                    "value": text(),
                    "key": text()
                }
            });
            json!({
                "project": text(),
                "scenario": text(),
                "url": text(),
                "revision": text(),
                "timeout_ms": {"type": "integer", "minimum": 1},
                "allow_degraded": {"type": "boolean"},
                "environment": {
                    "type": "object",
                    "properties": {
                        "width": {"type": "integer", "minimum": 1},
                        "height": {"type": "integer", "minimum": 1},
                        "scale": {"type": "number"},
                        "locale": text(),
                        "color_scheme": {"enum": ["light", "dark"]},
                        "reduced_motion": {"type": "boolean"}
                    }
                },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["kind"],
                        "properties": {
                            "kind": {"enum": ["click", "hover", "fill", "press", "ready", "checkpoint", "clip"]},
                            "selector": text(),
                            "value": text(),
                            "key": text(),
                            "id": text(),
                            "trigger": action,
                            "samples": {"type": "integer", "minimum": 2},
                            "interval_ms": {"type": "integer", "minimum": 16}
                        }
                    }
                }
            })
        },
    },
    Tool {
        name: "capture_capabilities",
        description: "Probe which server-side web capture backends are available and return OS install hints for missing ones",
        required: &[],
        properties: || json!({}),
    },
    Tool {
        name: "capture_review",
        description: "Leave a review note on a capture, optionally pointing at one of its artifacts",
        required: &["id", "note"],
        properties: || json!({"id": text(), "note": text(), "evidence_artifact_id": text()}),
    },
    Tool {
        name: "artifact_read",
        description: "Read a slice of a captured file as base64; page with offset and length",
        required: &["id"],
        properties: || json!({"id": text(), "offset": {"type": "integer", "minimum": 0}, "length": {"type": "integer", "minimum": 1, "maximum": MAX_READ_BYTES}}),
    },
    Tool {
        name: "problem_list",
        description: "List a project's problems, most recently updated first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "problem_get",
        description: "One problem with its rules, evidence and comments",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "problem_create",
        description: "File a problem; evidence lists artifact ids of the project's captures",
        required: &["project", "title", "severity", "origin"],
        properties: || {
            json!({"project": text(), "title": text(), "severity": severity(),
        "origin": {"enum": ["agent-measured", "agent-visual", "user-reported"]},
        "route": text(), "viewport": text(), "theme": text(), "diagnosis": text(), "proposed_solution": text(),
        "rules": {"type": "array", "items": {"type": "object", "required": ["scope", "code", "title"], "properties": {"scope": text(), "code": text(), "title": text()}}},
        "evidence": string_list()})
        },
    },
    Tool {
        name: "problem_triage",
        description: "Change a problem's status, severity or proposed solution",
        required: &["id"],
        properties: || {
            json!({"id": text(),
        "status": {"enum": ["open", "in-progress", "intentional", "false-positive", "needs-reprocess", "resolved"]},
        "severity": severity(), "proposed_solution": text()})
        },
    },
    Tool {
        name: "problem_comment",
        description: "Add a comment to a problem",
        required: &["id", "text"],
        properties: || json!({"id": text(), "text": text()}),
    },
    Tool {
        name: "problem_analyze",
        description: "Infer and file deduplicated, unsuppressed problems from a capture's findings",
        required: &["project", "capture"],
        properties: || json!({"project": text(), "capture": text()}),
    },
    Tool {
        name: "design_list",
        description: "List a project's design records (briefs, directions, tactics, decisions), newest first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "design_record",
        description: "Record a design brief, direction, tactic or decision; supersedes names the record it replaces",
        required: &["project", "kind", "status", "title"],
        properties: || {
            json!({"project": text(),
        "kind": {"enum": ["brief", "direction", "tactic", "decision"]},
        "status": {"enum": ["proposed", "adopted", "rejected"]},
        "title": text(), "note": text(), "surface": text(), "supersedes": text(),
        "reference": text(), "evidence": text()})
        },
    },
    Tool {
        name: "fix_list",
        description: "List a project's fixes, most recently updated first",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "fix_get",
        description: "One fix with its comparisons, steps and reviews",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "fix_create",
        description: "File a fix for one or more problems; they go in progress until the fix is approved",
        required: &["project", "title", "problems"],
        properties: || {
            json!({"project": text(), "title": text(), "summary": text(),
        "problems": string_list(), "tactics": string_list(), "files": string_list(),
        "comparisons": comparisons(),
        "steps": {"type": "array", "items": {"type": "object", "required": ["title"], "properties": step_properties()}}})
        },
    },
    Tool {
        name: "fix_step",
        description: "Record one more verified step of a fix",
        required: &["id", "title"],
        properties: || {
            let mut properties = step_properties();
            properties["id"] = text();
            properties
        },
    },
    Tool {
        name: "fix_review",
        description: "Approve a fix (resolves its problems), request changes, or mark it superseded",
        required: &["id", "outcome"],
        properties: || {
            json!({"id": text(),
        "outcome": {"enum": ["approved", "changes-requested", "superseded"]}, "comment": text()})
        },
    },
];

/// What `tools/list` returns.
fn catalog() -> Vec<Value> {
    TOOLS.iter().map(Tool::describe).collect()
}

/// A JSON-RPC error response body.
fn rpc_error(code: i64, message: &str) -> Value {
    json!({"error": {"code": code, "message": message}})
}

/// A tool result: the JSON as text content, flagged as an error when the call failed.
fn tool_result(outcome: Result<Value, ApiError>) -> Value {
    let (text, is_error) = match outcome {
        Ok(body) => (body.to_string(), false),
        Err(ApiError::Internal(err)) => {
            tracing::error!(error = %err, "mcp internal error");
            ("internal server error".to_owned(), true)
        }
        Err(error) => (error.to_string(), true),
    };
    json!({"result": {"content": [{"type": "text", "text": text}], "isError": is_error}})
}

/// The reply body for one request, without the `jsonrpc` and `id` envelope.
async fn dispatch(state: Shared, principal: Principal, request: &Value) -> Value {
    let params = &request["params"];
    match request["method"].as_str() {
        Some("initialize") => {
            let version = params["protocolVersion"]
                .as_str()
                .filter(|version| PROTOCOL_VERSIONS.contains(version))
                .unwrap_or(DEFAULT_PROTOCOL_VERSION);
            json!({"result": {"protocolVersion": version, "capabilities": {"tools": {}},
                "serverInfo": {"name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION")}}})
        }
        Some("ping") => json!({"result": {}}),
        Some("tools/list") => json!({"result": {"tools": catalog()}}),
        Some("tools/call") => {
            let mut call = params.clone();
            if call.get("arguments").is_none() {
                call["arguments"] = json!({});
            }
            match serde_json::from_value::<Call>(call) {
                Ok(call) => tool_result(call.run(state, principal).await),
                Err(error) => rpc_error(INVALID_PARAMS, &error.to_string()),
            }
        }
        _ => rpc_error(METHOD_NOT_FOUND, "Method not found"),
    }
}

/// `POST /mcp`
pub async fn serve(
    State(state): State<Shared>,
    principal: Principal,
    body: axum::body::Bytes,
) -> Response {
    let (id, mut reply) = match serde_json::from_slice::<Value>(&body) {
        Err(_) => (Value::Null, rpc_error(PARSE_ERROR, "Parse error")),
        Ok(request) if request["jsonrpc"] != "2.0" || !request["method"].is_string() => {
            (Value::Null, rpc_error(INVALID_REQUEST, "Invalid request"))
        }
        // A notification has no id and gets no reply.
        Ok(request) if request.get("id").is_none() => return StatusCode::ACCEPTED.into_response(),
        Ok(request) if !(request["id"].is_number() || request["id"].is_string()) => {
            (Value::Null, rpc_error(INVALID_REQUEST, "Invalid request"))
        }
        Ok(request) => (
            request["id"].clone(),
            dispatch(state, principal, &request).await,
        ),
    };
    reply["jsonrpc"] = json!("2.0");
    reply["id"] = id;
    Json(reply).into_response()
}

#[cfg(test)]
mod tests {
    use super::{Call, TOOLS};
    use crate::testkit::*;
    use axum::http::StatusCode;
    use base64::Engine as _;
    use serde_json::{Value, json};
    use sha2::Digest as _;

    /// Calls `/mcp` as `token` and returns the JSON-RPC reply.
    async fn rpc(port: u16, token: &str, body: &Value) -> Value {
        let response = call(port, "POST", "/mcp", Some(token), Some(&body.to_string())).await;
        assert_eq!(status(&response), StatusCode::OK, "{response}");
        json(&response)
    }

    fn tool(name: &str, arguments: &Value) -> Value {
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}})
    }

    /// The JSON a successful tool call returned.
    fn payload(reply: &Value) -> Value {
        assert_eq!(reply["result"]["isError"], false, "{reply}");
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn every_catalog_entry_is_a_call_variant() {
        for tool in TOOLS {
            let outcome =
                serde_json::from_value::<Call>(json!({"name": tool.name, "arguments": {}}));
            if let Err(error) = outcome {
                assert!(
                    !error.to_string().contains("unknown variant"),
                    "{} has no Call variant: {error}",
                    tool.name
                );
            }
        }
    }

    #[tokio::test]
    async fn an_agent_files_a_problem_and_a_fix_through_mcp() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        let listed = rpc(
            port,
            &ana,
            &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        )
        .await;
        assert_eq!(
            listed["result"]["tools"].as_array().unwrap().len(),
            TOOLS.len()
        );

        let problem = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "problem_create",
                    &json!({"project": "demo", "title": "Cramped", "severity": "minor",
                        "origin": "agent-visual", "evidence": [PROJECT_ARTIFACT]}),
                ),
            )
            .await,
        );
        let problem_id = problem["id"].as_str().unwrap();
        let fix = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "fix_create",
                    &json!({"project": "demo", "title": "Space", "problems": [problem_id]}),
                ),
            )
            .await,
        );
        let approved = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "fix_review",
                    &json!({"id": fix["id"], "outcome": "approved"}),
                ),
            )
            .await,
        );
        assert_eq!(approved["status"], "approved");
        let resolved = payload(
            &rpc(
                port,
                &bruno,
                &tool("problem_get", &json!({"id": problem_id})),
            )
            .await,
        );
        assert_eq!(resolved["status"], "resolved");
    }

    #[tokio::test]
    async fn tool_failures_are_results_and_bad_requests_are_rpc_errors() {
        let (port, [_, bruno, _]) = demo_with_artifacts().await;
        let denied = rpc(
            port,
            &bruno,
            &tool("problem_create", &json!({"project": "demo", "title": "x", "severity": "minor", "origin": "user-reported"})),
        )
        .await;
        assert_eq!(denied["result"]["isError"], true, "{denied}");
        let unknown = rpc(port, &bruno, &tool("nope", &json!({}))).await;
        assert!(unknown["error"]["code"].is_i64(), "{unknown}");
        let anonymous = call(port, "POST", "/mcp", None, Some("{}")).await;
        assert_eq!(status(&anonymous), StatusCode::UNAUTHORIZED, "{anonymous}");
    }

    /// Content uploaded in the artifact test, and the part of it read back.
    const CONTENT: &str = "hello-world";
    const READ_OFFSET: u64 = 6;
    const READ_LENGTH: u64 = 5;
    const READ_BACK: &str = "world";

    #[tokio::test]
    async fn artifacts_are_read_in_pages() {
        let (port, [ana, ..]) = demo_with_artifacts().await;
        let sha = format!("{:x}", sha2::Sha256::digest(CONTENT));
        let upload = request(
            port,
            format!(
                "PUT /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {ana}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{CONTENT}",
                CONTENT.len()
            ),
        )
        .await;
        assert!(status(&upload).is_success(), "{upload}");
        let capture = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&format!(
                r#"{{"scenario":"home","status":"complete","environment":{{}},
                    "checkpoints":[{{"id":"home","path":"home.png","sha256":"{sha}"}}]}}"#
            )),
        )
        .await;
        let capture: rupicola_api::Capture = json(&capture);
        let artifact = &capture.checkpoints[0].artifact.id;
        let read = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "artifact_read",
                    &json!({"id": artifact, "offset": READ_OFFSET, "length": READ_LENGTH}),
                ),
            )
            .await,
        );
        assert_eq!(read["length"], READ_LENGTH);
        assert_eq!(read["bytes"], CONTENT.len());
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(read["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(decoded, READ_BACK.as_bytes());
    }
}
