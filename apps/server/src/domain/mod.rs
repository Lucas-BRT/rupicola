//! The server's own rules, free of I/O: the errors a request can end in, who is
//! calling and what they may do, and the checks a value must pass before it is kept.
//! The shapes clients see live in `rupicola-api`; this layer only decides.

pub mod account;
pub mod blob;
pub mod capture;
pub mod credential;
pub mod design;
pub mod design_system;
pub mod error;
pub mod fix;
pub mod limits;
pub mod lineage;
pub mod principal;
pub mod problem;
pub mod project;
pub mod record;
pub mod removal;
pub mod search;
pub mod secret;
pub mod selection;
pub mod topic;

pub use error::ApiError;
pub use principal::{Access, Principal};
