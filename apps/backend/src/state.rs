//! Shared server state handed to every handler.

use crate::auth::{Lockout, Passwords};
use anyhow::Context;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;
use turso::Connection;

pub type Shared = Arc<AppState>;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub passwords: Passwords,
    pub lockout: Lockout,
    /// SHA-256 of the setup code that may create the first owner, if configured.
    pub setup_code: Option<String>,
    /// Whether the first owner may be created without a setup code (loopback only).
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
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let database =
            turso::Builder::new_local(path.to_str().context("database path is not UTF-8")?)
                .build()
                .await
                .with_context(|| format!("cannot open {}", path.display()))?;
        let db = crate::db::connect(&database).await?;
        crate::db::migrate(&db).await?;
        Ok(Arc::new(Self {
            db: Mutex::new(db),
            passwords: Passwords::new()?,
            lockout: Lockout::default(),
            setup_code: setup_code.map(crate::auth::digest),
            open_setup,
        }))
    }
}
