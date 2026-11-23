//! The server's own rules, free of I/O: the errors a request can end in, who is
//! calling and what they may do, and the checks a value must pass before it is kept.
//! The shapes clients see live in `rupicola-api`; this layer only decides.

pub mod account;
pub mod blob;
pub mod capture;
pub mod credential;
pub mod design;
pub mod error;
pub mod limits;
pub mod principal;
pub mod project;
pub mod secret;

pub use error::ApiError;
pub use principal::{Access, Principal};
