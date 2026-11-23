//! Shared server state handed to every handler.

use crate::{
    domain::limits::LimitDefaults,
    services::passwords::{Lockout, Passwords},
    storage::blobs::Blobs,
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
    /// Lets one browser validation of a design system run at a time.
    pub design_validation: tokio::sync::Semaphore,
    /// What uploads may be before any setting changes it.
    pub limits: LimitDefaults,
    /// Whether the server lets any project remove a cited capture.
    pub cited_deletion: std::sync::atomic::AtomicBool,
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
        limits: LimitDefaults,
    ) -> anyhow::Result<Shared> {
        let root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("blobs");
        let blobs = Blobs::disk(root.clone())
            .with_context(|| format!("cannot create {}", root.display()))?;
        Self::new(
            crate::repositories::open(path).await?,
            blobs,
            setup_code,
            open_setup,
            limits,
        )
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
        limits: LimitDefaults,
    ) -> anyhow::Result<Shared> {
        Ok(Arc::new(Self {
            db,
            blobs,
            passwords: Passwords::new()?,
            lockout: Lockout::default(),
            setup_code: setup_code.map(crate::domain::secret::digest),
            open_setup,
            design_validation: tokio::sync::Semaphore::new(1),
            limits,
            cited_deletion: std::sync::atomic::AtomicBool::new(true),
        }))
    }
}
