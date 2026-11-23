//! The use cases: each function checks who may act, applies the domain rules and
//! drives the repositories and the blob store, inside one transaction when it writes
//! more than one row. Both the HTTP handlers and the MCP tools call these.

pub mod access;
pub mod auth;
pub mod credentials;
pub mod design;
pub mod passwords;
pub mod projects;
