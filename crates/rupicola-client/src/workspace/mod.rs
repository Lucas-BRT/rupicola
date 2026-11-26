//! Token-bound client implementing all atomic `rupicola_api` operation traits
//! over any [`Api`] transport plus local `rupicola-core` offline analysis.
//!
//! Via blanket supertrait impls, `WorkspaceClient<A>` satisfies `WorkspaceOps`,
//! `AuditorOps`, `FullAgentOps`, `RunnerOps`, `AdminAuthOps`, `CliOps`, and `PanelOps`.

mod admin;
mod automation;
mod domain;
mod governance;
mod media;
#[cfg(test)]
mod tests;

use crate::{Api, ClientError, McpReply, McpTransport, Result};
use async_trait::async_trait;
use rupicola_api::OpsBase;
use serde_json::{Value, json};

/// A token-bound client that implements every atomic operation trait over any [`Api`]
/// transport plus local `rupicola-core` offline analysis.
#[derive(Clone, Debug)]
pub struct WorkspaceClient<A: Api> {
    pub(super) api: A,
    pub(super) token: String,
}

impl<A: Api> WorkspaceClient<A> {
    pub fn new(api: A, token: impl Into<String>) -> Self {
        Self {
            api,
            token: token.into(),
        }
    }

    pub const fn api(&self) -> &A {
        &self.api
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub(super) async fn call_mcp<T: serde::de::DeserializeOwned>(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<T> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        })
        .to_string();
        let reply = self.api.mcp(&self.token, &payload).await?;
        let parsed: Value =
            serde_json::from_str(&reply.body).map_err(|e| ClientError::Malformed(e.to_string()))?;
        if let Some(msg) = parsed["error"]["message"].as_str() {
            return Err(ClientError::Invalid(msg.to_owned()));
        }
        let text = parsed["result"]["content"][0]["text"]
            .as_str()
            .ok_or_else(|| ClientError::Malformed("missing MCP tool content".to_owned()))?;
        if parsed["result"]["isError"].as_bool() == Some(true) {
            return Err(ClientError::Invalid(text.to_owned()));
        }
        serde_json::from_str(text).map_err(|e| ClientError::Malformed(e.to_string()))
    }
}

impl<A: Api> OpsBase for WorkspaceClient<A> {
    type Error = ClientError;
}

#[async_trait]
impl<A: Api> McpTransport for WorkspaceClient<A> {
    async fn send(&self, payload: &str) -> Result<McpReply> {
        self.api.mcp(&self.token, payload).await
    }
}
