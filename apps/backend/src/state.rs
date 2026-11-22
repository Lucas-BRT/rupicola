//! Shared server state handed to every handler.

use crate::auth::{Lockout, Passwords};
use sqlx::SqlitePool;
use std::{path::Path, sync::Arc};

pub type Shared = Arc<AppState>;

pub struct AppState {
    pub db: SqlitePool,
    pub passwords: Passwords,
    pub lockout: Lockout,
    /// SHA-256 of the setup code that may create the first administrator, if set.
    pub setup_code: Option<String>,
    /// Whether the first administrator may be created without a setup code
    /// (only while listening on loopback).
    pub open_setup: bool,
}

impl AppState {
    /// Opens (and migrates) the database at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or migrated, or the
    /// password hasher cannot start.
    pub async fn open(
        path: &Path,
        setup_code: Option<&str>,
        open_setup: bool,
    ) -> anyhow::Result<Shared> {
        Ok(Arc::new(Self {
            db: crate::db::open(path).await?,
            passwords: Passwords::new()?,
            lockout: Lockout::default(),
            setup_code: setup_code.map(crate::auth::digest),
            open_setup,
        }))
    }
}
