//! The Model Context Protocol endpoint (`POST /mcp`): JSON-RPC 2.0 whose tools call the
//! same services as the REST API, so an agent has exactly the access its token gives.
//!
//! Tools are the variants of [`Call`]; [`catalog`] describes them. A test keeps the two
//! in step.

use crate::{
    domain::{ApiError, Principal},
    services::{captures, design, fixes, jobs, problems, projects, scenarios},
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
    NewFinding, NewFix, NewJob, NewProblem, NewProject, NewReview, NewStep, ProblemPatch, Scenario,
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
    ProblemDelete {
        id: String,
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
    JobList {
        project: String,
        /// Only the jobs still waiting or running.
        #[serde(default)]
        open: bool,
    },
    JobGet {
        id: String,
    },
    JobCreate {
        project: String,
        #[serde(flatten)]
        job: NewJob,
    },
    JobCancel {
        id: String,
    },
    ScenarioList {
        project: String,
    },
    ScenarioGet {
        project: String,
        name: String,
        /// One exact version; the latest when absent.
        #[serde(default)]
        hash: Option<String>,
    },
    ScenarioPut {
        project: String,
        scenario: Scenario,
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
            Self::CaptureCapabilities {} => value(&captures::capabilities().await),
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
            Self::ProblemDelete { id } => {
                problems::delete(&state, &principal, &id).await?;
                Ok(json!({"deleted": id}))
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
            Self::JobList { project, open } => {
                value(&jobs::list(&state, &principal, &project, open).await?)
            }
            Self::JobGet { id } => value(&jobs::get(&state, &principal, &id).await?),
            Self::JobCreate { project, job } => {
                value(&jobs::create(&state, &principal, &project, &job).await?)
            }
            Self::JobCancel { id } => value(&jobs::cancel(&state, &principal, &id).await?),
            Self::ScenarioList { project } => {
                value(&scenarios::list(&state, &principal, &project).await?)
            }
            Self::ScenarioGet {
                project,
                name,
                hash,
            } => match hash {
                Some(hash) => {
                    value(&scenarios::version(&state, &principal, &project, &name, &hash).await?)
                }
                None => value(&scenarios::latest(&state, &principal, &project, &name).await?),
            },
            Self::ScenarioPut { project, scenario } => value(
                &scenarios::put(&state, &principal, &project, &scenario)
                    .await?
                    .0,
            ),
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
            "changed_ratio": {"type": "number"},
            "view": {"enum": ["isolated", "integrated"],
                "description": "isolated: the base against the base plus this fix alone. integrated: the base with every approved fix against the same plus this fix."},
            "base_revision": text(), "revision": text(),
            "applied_fixes": {"type": "array", "items": text(),
                "description": "Ids of the approved fixes the before tree already had; empty when isolated."}}}})
}

fn step_properties() -> Value {
    json!({"title": text(), "detail": text(), "verification": text(), "evidence": text()})
}

fn selections_schema() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "required": ["label", "items"],
            "properties": {
                "label": text(),
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["capture", "mark"],
                        "properties": {
                            "capture": text(),
                            "artifact": text(),
                            "path": text(),
                            "mark": {"enum": ["proof", "context"]},
                            "fragment": {"type": "object"}
                        }
                    }
                }
            }
        }
    })
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
        name: "capture_capabilities",
        description: "Probe which host OS capture backends are available and return install hints for missing ones",
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
        description: "File a problem; requires at least one evidence artifact id from the project's captures",
        required: &["project", "title", "severity", "origin", "evidence"],
        properties: || {
            json!({"project": text(), "title": text(), "severity": severity(),
        "origin": {"enum": ["agent-measured", "agent-visual", "user-reported"]},
        "route": text(), "viewport": text(), "theme": text(), "diagnosis": text(), "proposed_solution": text(),
        "rules": {"type": "array", "items": {"type": "object", "required": ["scope", "code", "title"], "properties": {"scope": text(), "code": text(), "title": text()}}},
        "evidence": {"type": "array", "minItems": 1, "items": text()}, "selections": selections_schema()})
        },
    },
    Tool {
        name: "problem_delete",
        description: "Permanently delete a problem from its project",
        required: &["id"],
        properties: || json!({"id": text()}),
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
            json!({"project": text(), "title": text(), "summary": text(), "git_ref": text(),
        "problems": string_list(), "tactics": string_list(), "files": string_list(),
        "comparisons": comparisons(), "baseline": selections_schema(), "proof": selections_schema(),
        "steps": {"type": "array", "items": {"type": "object", "required": ["title"], "properties": step_properties()}}})
        },
    },
    Tool {
        name: "job_create",
        description: "Queue a capture job: run a scenario on one or more trees of the code. A runner takes it, builds each tree (a git ref with other refs merged on top), captures the scenario's matrix and uploads the captures. Poll job_get for cells_done of cells_total",
        required: &["project", "scenario", "trees"],
        properties: || {
            json!({"project": text(), "scenario": text(),
                "hash": {"type": "string", "description": "Exact scenario version; the latest when absent."},
                "fix": {"type": "string", "description": "The fix the captures are for."},
                "trees": {"type": "array", "items": {"type": "object", "required": ["name", "base"],
                    "properties": {"name": text(), "base": {"type": "string", "description": "Git ref to check out."},
                        "merge": string_list(), "fixes": string_list()}}}})
        },
    },
    Tool {
        name: "job_list",
        description: "List a project's capture jobs, newest first; open=true keeps the ones waiting or running",
        required: &["project"],
        properties: || json!({"project": text(), "open": {"type": "boolean"}}),
    },
    Tool {
        name: "job_get",
        description: "One capture job: status, cells_done of cells_total, the runner, and the captures it made",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "job_cancel",
        description: "Cancel a capture job that has not finished; a runner holding it stops at its next report",
        required: &["id"],
        properties: || json!({"id": text()}),
    },
    Tool {
        name: "scenario_list",
        description: "List a project's scenarios, the capture definitions its repository owns, latest version of each",
        required: &["project"],
        properties: || json!({"project": text()}),
    },
    Tool {
        name: "scenario_get",
        description: "One scenario definition: its latest version, or the exact version with `hash`",
        required: &["project", "name"],
        properties: || json!({"project": text(), "name": text(), "hash": text()}),
    },
    Tool {
        name: "scenario_put",
        description: "Push a scenario definition. The repository's TOML file is the source of truth; this keeps a copy under the hash of the parsed definition. Same definition twice is a no-op",
        required: &["project", "scenario"],
        properties: || {
            json!({"project": text(),
                "scenario": {"type": "object", "required": ["name", "url"],
                    "description": "The parsed .rupicola/scenarios/*.toml: name, url, environment, matrix {viewports, themes}, themes, steps, checks.",
                    "properties": {"name": text(), "url": text(), "environment": {"type": "object"},
                        "matrix": {"type": "object"}, "themes": {"type": "object"},
                        "steps": {"type": "array"}, "checks": {"type": "object"}}}})
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
    async fn an_agent_queues_watches_and_cancels_a_capture_job_through_mcp() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "scenario_put",
                    &json!({"project": "demo", "scenario": {"name": "home", "url": "/"}}),
                ),
            )
            .await,
        );
        let job = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "job_create",
                    &json!({"project": "demo", "scenario": "home",
                        "trees": [{"name": "base", "base": "develop"}]}),
                ),
            )
            .await,
        );
        assert_eq!(
            (job["status"].as_str(), job["cells_total"].as_u64()),
            (Some("queued"), Some(1))
        );
        let open = payload(
            &rpc(
                port,
                &bruno,
                &tool("job_list", &json!({"project": "demo", "open": true})),
            )
            .await,
        );
        assert_eq!(open.as_array().unwrap().len(), 1);
        let cancelled =
            payload(&rpc(port, &ana, &tool("job_cancel", &json!({"id": job["id"]}))).await);
        assert_eq!(cancelled["status"], "cancelled");
        let fetched =
            payload(&rpc(port, &bruno, &tool("job_get", &json!({"id": job["id"]}))).await);
        assert_eq!(fetched["status"], "cancelled");
    }

    #[tokio::test]
    async fn an_agent_pushes_and_reads_a_scenario_through_mcp() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        let scenario = json!({"name": "home", "url": "/",
            "matrix": {"viewports": ["390x844@2"], "themes": ["dark", "light"]}});
        let put = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "scenario_put",
                    &json!({"project": "demo", "scenario": scenario}),
                ),
            )
            .await,
        );
        let hash = put["hash"].as_str().unwrap().to_owned();
        let listed = payload(
            &rpc(
                port,
                &bruno,
                &tool("scenario_list", &json!({"project": "demo"})),
            )
            .await,
        );
        assert_eq!(
            (listed[0]["name"].as_str(), listed[0]["cells"].as_u64()),
            (Some("home"), Some(2))
        );
        let exact = payload(
            &rpc(
                port,
                &bruno,
                &tool(
                    "scenario_get",
                    &json!({"project": "demo", "name": "home", "hash": hash}),
                ),
            )
            .await,
        );
        assert_eq!(exact["scenario"]["url"], "/");
        let latest = payload(
            &rpc(
                port,
                &bruno,
                &tool("scenario_get", &json!({"project": "demo", "name": "home"})),
            )
            .await,
        );
        assert_eq!(latest["hash"], exact["hash"]);
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
        let sha = stream_blob(port, &ana, "demo", CONTENT.as_bytes()).await;
        let record = capture_record("cap-mcp-read", &sha, CONTENT.len() as u64, vec![]);
        let capture = publish_record(port, &ana, &record).await;
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

    #[tokio::test]
    async fn capture_capabilities_and_selection_schemas_work() {
        const CAPTURE_SHOT: &str = "capture-shot";
        let (port, [ana, ..]) = demo_with_artifacts().await;
        let caps = payload(&rpc(port, &ana, &tool("capture_capabilities", &json!({}))).await);
        assert!(caps["backends"].is_array(), "{caps}");

        let selection = json!([{
            "label": "Proof",
            "items": [{"capture": CAPTURE_SHOT, "artifact": PROJECT_ARTIFACT, "mark": "proof"}]
        }]);
        let problem = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "problem_create",
                    &json!({"project": "demo", "title": "Alignment", "severity": "minor",
                        "origin": "agent-visual", "selections": selection}),
                ),
            )
            .await,
        );
        assert_eq!(problem["selections"].as_array().unwrap().len(), 1);

        let fix = payload(
            &rpc(
                port,
                &ana,
                &tool(
                    "fix_create",
                    &json!({"project": "demo", "title": "Aligned", "problems": [problem["id"]],
                        "baseline": selection, "proof": selection}),
                ),
            )
            .await,
        );
        assert_eq!(fix["selections"].as_array().unwrap().len(), 2);
    }
}
