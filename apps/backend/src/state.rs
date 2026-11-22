//! Shared server state handed to every handler.

use crate::{
    auth::{Lockout, Passwords},
    blobs::Blobs,
};
use anyhow::Context;
use sqlx::SqlitePool;
use std::{path::Path, sync::Arc};

pub type Shared = Arc<AppState>;

pub struct AppState {
    pub db: SqlitePool,
    pub blobs: Blobs,
    pub passwords: Passwords,
    pub lockout: Lockout,
    /// SHA-256 of the setup code that may create the first administrator, if set.
    pub setup_code: Option<String>,
    /// Whether the first administrator may be created without a setup code
    /// (only while listening on loopback).
    pub open_setup: bool,
}

impl AppState {
    /// Opens (and migrates) the database at `path`; media blobs live in `blobs/`
    /// next to it.
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
        let root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("blobs");
        let blobs = Blobs::disk(root.clone())
            .with_context(|| format!("cannot create {}", root.display()))?;
        Self::new(crate::db::open(path).await?, blobs, setup_code, open_setup)
    }

    /// Builds the state around an already migrated pool.
    ///
    /// # Errors
    ///
    /// Returns an error if the password hasher cannot start.
    pub fn new(
        db: SqlitePool,
        blobs: Blobs,
        setup_code: Option<&str>,
        open_setup: bool,
    ) -> anyhow::Result<Shared> {
        Ok(Arc::new(Self {
            db,
            blobs,
            passwords: Passwords::new()?,
            lockout: Lockout::default(),
            setup_code: setup_code.map(crate::auth::digest),
            open_setup,
        }))
    }
}
