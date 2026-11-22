//! Accounts and credentials: password sign-in, bearer tokens and the first-owner
//! setup. Every credential is a row in `tokens`, stored as the SHA-256 of its secret.

use crate::{error::ApiError, state::Shared};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{
        SaltString,
        rand_core::{OsRng, RngCore},
    },
};
use axum::{
    Json,
    extract::{FromRequestParts, State},
    http::{StatusCode, header, request::Parts},
};
use rupicola_api::{Credentials, Me, SessionToken, Setup, TokenKind};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, Semaphore};

/// How long a sign-in stays valid.
pub const SESSION_AGE: i64 = 7 * 24 * 60 * 60;
/// Failed sign-ins per account before it is locked for [`LOCK_SECS`].
const MAX_FAILURES: u32 = 8;
const LOCK_SECS: i64 = 300;

pub fn now() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    i64::try_from(secs).unwrap_or(i64::MAX)
}

/// A new 256-bit secret, hex encoded.
pub fn token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

pub fn digest(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

fn valid_handle(handle: &str) -> bool {
    (3..=48).contains(&handle.len())
        && handle
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Argon2 hashing off the async threads, with bounded parallelism. Unknown accounts
/// are verified against a dummy hash so timing does not reveal which handles exist.
pub struct Passwords {
    work: std::sync::Arc<Semaphore>,
    dummy: String,
}

impl Passwords {
    /// # Errors
    ///
    /// Returns an error if the dummy hash cannot be computed.
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            work: std::sync::Arc::new(Semaphore::new(4)),
            dummy: hash(&token())?,
        })
    }

    async fn hash(&self, password: String) -> Result<String, ApiError> {
        let permit = self.permit()?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hash(&password)
        })
        .await
        .map_err(anyhow::Error::from)?
        .map_err(ApiError::Internal)
    }

    async fn verify(&self, stored: Option<String>, password: String) -> Result<bool, ApiError> {
        let exists = stored.is_some();
        let stored = stored.unwrap_or_else(|| self.dummy.clone());
        let permit = self.permit()?;
        let matches = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            PasswordHash::new(&stored).is_ok_and(|hash| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            })
        })
        .await
        .map_err(anyhow::Error::from)?;
        Ok(exists && matches)
    }

    fn permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, ApiError> {
        self.work
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError::Overloaded)
    }
}

fn hash(password: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|error| anyhow::anyhow!("password hashing failed: {error}"))
}

/// Per-account failed sign-in counter, so guessing one account's password is slow
/// even from many addresses.
#[derive(Default)]
pub struct Lockout(Mutex<HashMap<String, (u32, i64)>>);

impl Lockout {
    async fn retry_after(&self, handle: &str) -> Option<u64> {
        let mut entries = self.0.lock().await;
        let current = now();
        entries.retain(|_, (_, since)| current - *since < LOCK_SECS);
        entries
            .get(handle)
            .filter(|(failures, _)| *failures >= MAX_FAILURES)
            .map(|(_, since)| u64::try_from(LOCK_SECS - (current - since)).unwrap_or(1))
    }

    async fn fail(&self, handle: &str) {
        let mut entries = self.0.lock().await;
        let entry = entries.entry(handle.to_owned()).or_insert((0, now()));
        entry.0 = entry.0.saturating_add(1);
    }

    async fn clear(&self, handle: &str) {
        self.0.lock().await.remove(handle);
    }
}

/// The verified owner of the bearer token on a request.
pub struct Principal {
    pub kind: TokenKind,
    pub handle: Option<String>,
    pub owner: bool,
    pub project: Option<String>,
    digest: String,
}

impl FromRequestParts<Shared> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, ApiError> {
        let secret = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|secret| secret.len() == 64 && secret.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(ApiError::Unauthorized)?;
        let digest = digest(secret);
        let db = state.db.lock().await;
        let mut rows = db
            .query(
                "SELECT t.kind, t.handle, t.project, COALESCE(a.owner, 0) FROM tokens t
                 LEFT JOIN accounts a ON a.handle = t.handle
                 WHERE t.digest = ?1 AND (t.expires IS NULL OR t.expires > ?2)
                   AND (t.handle IS NULL OR a.handle IS NOT NULL)",
                turso::params![digest.as_str(), now()],
            )
            .await?;
        let row = rows.next().await?.ok_or(ApiError::Unauthorized)?;
        let kind = match row.get::<String>(0)?.as_str() {
            "session" => TokenKind::Session,
            "machine" => TokenKind::Machine,
            "device" => TokenKind::Device,
            _ => return Err(ApiError::Unauthorized),
        };
        let principal = Self {
            kind,
            handle: row.get(1)?,
            project: row.get(2)?,
            owner: row.get::<i64>(3)? == 1,
            digest,
        };
        drop(rows);
        db.execute(
            "UPDATE tokens SET last_seen = ?1 WHERE digest = ?2",
            turso::params![now(), principal.digest.as_str()],
        )
        .await?;
        Ok(principal)
    }
}

/// `POST /v1/sessions`
pub async fn sign_in(
    State(state): State<Shared>,
    Json(form): Json<Credentials>,
) -> Result<(StatusCode, Json<SessionToken>), ApiError> {
    let handle = form.handle.trim().to_ascii_lowercase();
    if let Some(retry_after) = state.lockout.retry_after(&handle).await {
        return Err(ApiError::TooManyRequests { retry_after });
    }
    let stored = if valid_handle(&handle) && form.password.len() <= 1024 {
        let db = state.db.lock().await;
        let mut rows = db
            .query(
                "SELECT password FROM accounts WHERE handle = ?1",
                [handle.as_str()],
            )
            .await?;
        rows.next()
            .await?
            .map(|row| row.get::<String>(0))
            .transpose()?
    } else {
        None
    };
    if !state.passwords.verify(stored, form.password).await? {
        state.lockout.fail(&handle).await;
        return Err(ApiError::Unauthorized);
    }
    state.lockout.clear(&handle).await;
    Ok((
        StatusCode::CREATED,
        Json(issue_session(&state, &handle).await?),
    ))
}

async fn issue_session(state: &Shared, handle: &str) -> Result<SessionToken, ApiError> {
    let secret = token();
    let expires_at = now() + SESSION_AGE;
    let db = state.db.lock().await;
    db.execute(
        "DELETE FROM tokens WHERE expires IS NOT NULL AND expires <= ?1",
        [now()],
    )
    .await?;
    db.execute(
        "INSERT INTO tokens (digest, kind, handle, project, label, created, expires, last_seen)
         VALUES (?1, 'session', ?2, NULL, 'Session', ?3, ?4, NULL)",
        turso::params![digest(&secret), handle, now(), expires_at],
    )
    .await?;
    Ok(SessionToken {
        token: secret,
        expires_at,
    })
}

/// `DELETE /v1/sessions/current`
pub async fn sign_out(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<StatusCode, ApiError> {
    if principal.kind != TokenKind::Session {
        return Err(ApiError::Forbidden);
    }
    let db = state.db.lock().await;
    db.execute(
        "DELETE FROM tokens WHERE digest = ?1",
        [principal.digest.as_str()],
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/me`
pub async fn me(principal: Principal) -> Json<Me> {
    Json(Me {
        kind: principal.kind,
        handle: principal.handle,
        owner: principal.owner,
        project: principal.project,
    })
}

/// `POST /v1/setup`: create the first owner and sign them in. Refused once an owner
/// exists, and without the setup code unless the backend only listens on loopback.
pub async fn setup(
    State(state): State<Shared>,
    Json(form): Json<Setup>,
) -> Result<(StatusCode, Json<SessionToken>), ApiError> {
    let handle = form.handle.trim().to_ascii_lowercase();
    if let Some(retry_after) = state.lockout.retry_after("__setup__").await {
        return Err(ApiError::TooManyRequests { retry_after });
    }
    let code_ok = match (&state.setup_code, form.code.as_deref()) {
        (Some(expected), Some(code)) => {
            bool::from(expected.as_bytes().ct_eq(digest(code.trim()).as_bytes()))
        }
        (Some(_), None) => false,
        (None, _) => state.open_setup,
    };
    if !code_ok {
        state.lockout.fail("__setup__").await;
        return Err(ApiError::Forbidden);
    }
    if !valid_handle(&handle) || !(12..=1024).contains(&form.password.len()) {
        return Err(ApiError::Invalid(
            "use a 3–48 character username (letters, digits, - or _) and a password of at least 12 characters",
        ));
    }
    let password = state.passwords.hash(form.password).await?;
    let created = state
        .db
        .lock()
        .await
        .execute(
            "INSERT OR IGNORE INTO accounts SELECT ?1, ?2, 1
             WHERE NOT EXISTS (SELECT 1 FROM accounts WHERE owner = 1)",
            [handle.as_str(), password.as_str()],
        )
        .await?;
    if created == 0 {
        return Err(ApiError::Conflict("an owner already exists"));
    }
    tracing::info!(handle, "created the first owner account");
    Ok((
        StatusCode::CREATED,
        Json(issue_session(&state, &handle).await?),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_account_locks_after_repeated_failures() {
        let lockout = Lockout::default();
        for _ in 0..MAX_FAILURES - 1 {
            lockout.fail("ana").await;
        }
        assert!(lockout.retry_after("ana").await.is_none());
        lockout.fail("ana").await;
        assert!(lockout.retry_after("ana").await.is_some());
        assert!(lockout.retry_after("bruno").await.is_none());
        lockout.clear("ana").await;
        assert!(lockout.retry_after("ana").await.is_none());
    }

    #[test]
    fn tokens_are_random_hex_and_stored_only_as_digests() {
        let (first, second) = (token(), token());
        assert_eq!(first.len(), 64);
        assert_ne!(first, second);
        assert_eq!(digest(&first).len(), 64);
        assert_ne!(digest(&first), first);
    }
}
