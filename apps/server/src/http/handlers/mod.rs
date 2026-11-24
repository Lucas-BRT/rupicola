//! One file per resource; each handler extracts the request, calls its use case and
//! shapes the response.

pub mod account;
pub mod captures;
pub mod credentials;
pub mod design;
pub mod design_systems;
pub mod fixes;
pub mod jobs;
pub mod limits;
pub mod lineage;
pub mod problems;
pub mod projects;
pub mod publish;
pub mod removal;
pub mod scenarios;
pub mod search;
pub mod selections;
pub mod settings;
pub mod topics;
