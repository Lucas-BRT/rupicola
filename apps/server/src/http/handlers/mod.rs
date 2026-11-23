//! One file per resource; each handler extracts the request, calls its use case and
//! shapes the response.

pub mod account;
pub mod captures;
pub mod credentials;
pub mod design;
pub mod limits;
pub mod lineage;
pub mod projects;
pub mod publish;
pub mod settings;
