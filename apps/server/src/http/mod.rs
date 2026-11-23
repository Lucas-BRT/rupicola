//! The HTTP adapter: routes, the handlers they call, request extractors, error
//! responses, the hardening every request passes through, and the MCP endpoint.
//! Handlers translate between HTTP and the use cases in [`crate::services`]; they
//! hold no rules of their own.

mod error;
mod extract;
mod handlers;
pub mod mcp;
mod routes;
pub mod security;

#[cfg(test)]
pub use routes::SIGN_IN_BURST;
pub use routes::{HEALTH_PATH, app};
