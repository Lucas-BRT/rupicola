//! Serves Rupicola's tools to AI agents over the Model Context Protocol (stdio).

mod bundle;
mod call;
mod capture;
mod catalog;
mod design;
mod preview;
mod propose;
mod records;
mod server;

pub use server::serve;

/// JSON-RPC 2.0 error codes: the spec's parse error and unknown method, and the
/// server-defined codes the bridge uses for transport and HTTP failures.
pub(crate) const JSONRPC_PARSE_ERROR: i64 = -32700;
pub(crate) const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
pub(crate) const JSONRPC_TRANSPORT_ERROR: i64 = -32000;
pub(crate) const JSONRPC_HTTP_ERROR: i64 = -32001;
