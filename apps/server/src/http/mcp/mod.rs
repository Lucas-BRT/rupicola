//! The Model Context Protocol endpoint (`POST /mcp`): JSON-RPC 2.0 whose tools call the
//! same services as the REST API, so an agent has exactly the access its token gives.
//!
//! Tools are the variants of [`Call`]; [`catalog`] describes them. A test keeps the two
//! in step.

mod call;
mod catalog;
#[cfg(test)]
mod p0_tests;
#[cfg(test)]
mod p1_tests;
#[cfg(test)]
mod tests;

use self::call::Call;
#[cfg(test)]
use self::catalog::TOOLS;
use self::catalog::catalog;
use crate::{
    domain::{ApiError, Principal},
    state::Shared,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
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
