//! Which backend this machine talks to and the token that acts for it.
//!
//! `rupicola login` stores both under the data directory; a flag or an environment variable
//! overrides them for one run.

use crate::service;
use anyhow::{Context, Result, bail};
use rupicola_client::{ClientError, CredentialStore, HttpApi, StoredSession};
use std::{
    env, fs,
    io::{ErrorKind, Write},
    path::PathBuf,
};

/// Environment variable naming the backend, so no address is baked into the binary.
pub const SERVER_URL_ENV: &str = "RUPICOLA_SERVER_URL";
/// Environment variable holding a token when none is stored.
const TOKEN_ENV: &str = "RUPICOLA_MCP_TOKEN";
/// File holding the token, one line.
const TOKEN_FILE: &str = "auth-token";
/// File holding the backend's base URL, one line.
const SERVER_FILE: &str = "server-url";
/// Path of the MCP endpoint, which clients may be configured with instead of the base URL.
const MCP_SUFFIX: &str = "/mcp";
/// Permissions of the token file: the owner only.
#[cfg(unix)]
const PRIVATE_MODE: u32 = 0o600;
const NO_SERVER: &str =
    "no backend: pass --server-url, set RUPICOLA_SERVER_URL or run `rupicola login`";

/// The backend named on the command line or in the environment, else the stored one.
#[derive(clap::Args, Clone)]
pub struct Server {
    /// Base URL of the Rupicola backend (a trailing /mcp is accepted). Defaults to the one
    /// `rupicola login` stored.
    #[arg(long, env = SERVER_URL_ENV)]
    server_url: Option<String>,
    /// Whether the stored backend stands in when none is named.
    #[arg(skip = true)]
    use_stored: bool,
}

impl Server {
    /// A backend named outright, ignoring what is stored.
    #[cfg(test)]
    pub fn at(url: &str) -> Self {
        Self {
            server_url: Some(url.to_owned()),
            use_stored: false,
        }
    }

    /// No backend, and the stored one does not stand in.
    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            server_url: None,
            use_stored: false,
        }
    }

    /// The backend's base URL, without a trailing slash or `/mcp`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// Fails when the data directory cannot be resolved.
    pub fn url(&self) -> Result<Option<String>> {
        let named = match &self.server_url {
            Some(url) => Some(url.clone()),
            None if self.use_stored => Store::open()?.server_url()?,
            None => None,
        };
        Ok(named.map(|url| {
            let base = url.trim().trim_end_matches('/');
            base.strip_suffix(MCP_SUFFIX).unwrap_or(base).to_owned()
        }))
    }

    /// The backend's URL, or an error saying how to name one.
    ///
    /// # Errors
    ///
    /// Fails when no backend is named or stored.
    pub fn require_url(&self) -> Result<String> {
        self.url()?.context(NO_SERVER)
    }

    /// A connection to the backend, using the stored or environment token.
    ///
    /// # Errors
    ///
    /// Fails when there is no backend or token, or the HTTP client cannot be built.
    pub fn connect(&self) -> Result<Remote> {
        Remote::connect(&self.require_url()?)
    }
}

/// A backend and the token that acts for this machine.
pub struct Remote {
    pub api: HttpApi,
    pub token: String,
}

impl Remote {
    /// A connection to the backend at `base_url`, using the token from [`Store::token`].
    ///
    /// # Errors
    ///
    /// Fails when no token is stored or the HTTP client cannot be built.
    pub fn connect(base_url: &str) -> Result<Self> {
        Ok(Self {
            api: HttpApi::new(base_url)?,
            token: Store::open()?.token()?,
        })
    }
}

/// The files `rupicola login` keeps in the data directory.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// The store in the data directory.
    ///
    /// # Errors
    ///
    /// Fails when the data directory cannot be resolved.
    pub fn open() -> Result<Self> {
        Ok(Self {
            dir: service::data()?,
        })
    }

    pub fn token_path(&self) -> PathBuf {
        self.dir.join(TOKEN_FILE)
    }

    fn server_path(&self) -> PathBuf {
        self.dir.join(SERVER_FILE)
    }

    /// The stored token, else the one in the environment.
    ///
    /// # Errors
    ///
    /// Fails when neither holds a well-formed token.
    pub fn token(&self) -> Result<String> {
        let path = self.token_path();
        if let Ok(raw) = fs::read_to_string(&path)
            && !raw.trim().is_empty()
        {
            return Self::validate(&raw);
        }
        if let Ok(token) = env::var(TOKEN_ENV)
            && !token.trim().is_empty()
        {
            return Self::validate(&token);
        }
        bail!(
            "Cannot read {} and {TOKEN_ENV} is unset; run `rupicola login`",
            path.display()
        );
    }

    /// The stored backend URL, if any.
    ///
    /// # Errors
    ///
    /// Fails when the file exists but cannot be read.
    pub fn server_url(&self) -> Result<Option<String>> {
        let path = self.server_path();
        match fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text.trim().to_owned()).filter(|url| !url.is_empty())),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("Read {}", path.display())),
        }
    }

    /// Stores `token` for later commands, readable by the owner only.
    ///
    /// # Errors
    ///
    /// Fails when `token` is malformed or the file cannot be written.
    pub fn save_token(&self, token: &str) -> Result<()> {
        let token = Self::validate(token)?;
        self.write(&self.token_path(), &token)
    }

    /// Stores the backend's base URL.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be written.
    pub fn save_server_url(&self, url: &str) -> Result<()> {
        self.write(&self.server_path(), url)
    }

    /// Forgets the token and the backend; returns whether there was a token.
    ///
    /// # Errors
    ///
    /// Fails when a file exists but cannot be removed.
    pub fn clear(&self) -> Result<bool> {
        let had_token = Self::remove(&self.token_path())?;
        Self::remove(&self.server_path())?;
        Ok(had_token)
    }
}

impl CredentialStore for Store {
    fn load(&self) -> rupicola_client::Result<StoredSession> {
        let server_url = self
            .server_url()
            .map_err(|e| ClientError::Invalid(e.to_string()))?;
        let token = self.token().ok();
        Ok(StoredSession { server_url, token })
    }

    fn save(&self, server_url: Option<&str>, token: Option<&str>) -> rupicola_client::Result<()> {
        if let Some(url) = server_url {
            self.save_server_url(url)
                .map_err(|e| ClientError::Invalid(e.to_string()))?;
        }
        if let Some(tok) = token {
            self.save_token(tok)
                .map_err(|e| ClientError::Invalid(e.to_string()))?;
        }
        Ok(())
    }

    fn clear(&self) -> rupicola_client::Result<()> {
        Self::clear(self)
            .map(|_| ())
            .map_err(|e| ClientError::Invalid(e.to_string()))
    }
}

impl Store {
    fn remove(path: &PathBuf) -> Result<bool> {
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).with_context(|| format!("Remove {}", path.display())),
        }
    }

    fn validate(raw: &str) -> Result<String> {
        let token = raw.trim();
        if !token.is_empty() && token.bytes().all(|b| b.is_ascii_graphic()) {
            Ok(token.to_owned())
        } else {
            bail!("Rupicola auth token must be non-empty printable ASCII without whitespace");
        }
    }

    fn write(&self, path: &PathBuf, line: &str) -> Result<()> {
        fs::create_dir_all(&self.dir).with_context(|| format!("Create {}", self.dir.display()))?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(PRIVATE_MODE);
        }
        let mut file = options
            .open(path)
            .with_context(|| format!("Write {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_MODE))
                .with_context(|| format!("Set permissions on {}", path.display()))?;
        }
        file.write_all(line.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .with_context(|| format!("Write {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const URL: &str = "https://atelier.example.com";

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store {
            dir: dir.path().join("rupicola"),
        };
        (dir, store)
    }

    #[test]
    fn a_saved_token_and_backend_are_read_back_then_forgotten() {
        let (_dir, store) = store();
        assert_eq!(store.server_url().unwrap(), None);
        store.save_token(TOKEN).unwrap();
        store.save_server_url(URL).unwrap();
        assert_eq!(store.token().unwrap(), TOKEN);
        assert_eq!(store.server_url().unwrap().as_deref(), Some(URL));
        assert!(store.clear().unwrap());
        assert_eq!(store.server_url().unwrap(), None);
        assert!(!store.clear().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn the_token_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        const PERMISSIVE_MODE: u32 = 0o644;
        let (_dir, store) = store();
        store.save_token(TOKEN).unwrap();
        fs::set_permissions(
            store.token_path(),
            fs::Permissions::from_mode(PERMISSIVE_MODE),
        )
        .unwrap();
        store.save_token("opaque-token-v2").unwrap();
        let mode = fs::metadata(store.token_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, PRIVATE_MODE);
    }

    #[test]
    fn a_malformed_token_is_refused() {
        let (_dir, store) = store();
        for bad in ["", "   ", "has space", "has\nnewline", "has\x00null"] {
            assert!(store.save_token(bad).is_err());
        }
        assert!(!store.token_path().exists());
    }

    #[test]
    fn the_url_loses_a_trailing_slash_and_the_mcp_path() {
        for given in [
            URL,
            "https://atelier.example.com/",
            "https://atelier.example.com/mcp",
        ] {
            assert_eq!(Server::at(given).url().unwrap().as_deref(), Some(URL));
        }
        assert_eq!(Server::none().url().unwrap(), None);
        assert!(Server::none().require_url().is_err());
    }

    #[test]
    fn credential_store_trait_saves_loads_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store {
            dir: dir.path().to_path_buf(),
        };
        let cs: &dyn CredentialStore = &store;
        cs.save(Some(URL), Some("abc123token")).unwrap();
        let loaded = cs.load().unwrap();
        assert_eq!(loaded.server_url.as_deref(), Some(URL));
        assert_eq!(loaded.token.as_deref(), Some("abc123token"));
        cs.clear().unwrap();
        assert_eq!(cs.load().unwrap().server_url, None);
    }
}
