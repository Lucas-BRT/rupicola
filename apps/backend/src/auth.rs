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
use rupicola_api::{Credentials, Me, SessionToken, Setup, SetupStatus, TokenKind};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, Semaphore};

/// How long a sign-in stays valid, in milliseconds (7 days).
pub const SESSION_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Failed sign-ins per account before it is locked for [`LOCK_MS`].
const MAX_FAILURES: u32 = 8;
/// How long a locked account stays locked, in milliseconds (5 minutes).
const LOCK_MS: i64 = 5 * 60 * 1000;
/// Milliseconds in a second, to round lock waits up to whole seconds.
const MS_PER_SEC: u64 = 1000;
/// Random bytes in every secret (sessions, tokens, invitation codes).
const SECRET_BYTES: usize = 32;
/// Length of a secret or SHA-256 digest written as hex.
pub(crate) const HEX_SECRET_LEN: usize = 64;
/// Shortest and longest accepted username.
const HANDLE_LEN: std::ops::RangeInclusive<usize> = 3..=48;
/// Shortest and longest accepted password, in bytes.
pub(crate) const PASSWORD_LEN: std::ops::RangeInclusive<usize> = 12..=1024;
/// Argon2 hashes computed at once; more requests get 503 instead of piling up.
const PASSWORD_WORKERS: usize = 4;
/// Lockout key shared by setup attempts, which have no account yet.
const SETUP_LOCKOUT_KEY: &str = "__setup__";

/// Current time in Unix milliseconds.
pub fn now_ms() -> i64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

/// A new 256-bit secret, hex encoded.
pub fn token() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .fold(String::with_capacity(HEX_SECRET_LEN), |mut hex, byte| {
            use std::fmt::Write;
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

pub fn digest(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

/// The 422 sent for a username or password outside the accepted rules.
pub(crate) fn account_rules() -> ApiError {
    ApiError::invalid(format!(
        "use a {}–{} character username (letters, digits, - or _) and a password of {}–{} bytes",
        HANDLE_LEN.start(),
        HANDLE_LEN.end(),
        PASSWORD_LEN.start(),
        PASSWORD_LEN.end()
    ))
}

pub(crate) fn valid_handle(handle: &str) -> bool {
    HANDLE_LEN.contains(&handle.len())
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
            work: std::sync::Arc::new(Semaphore::new(PASSWORD_WORKERS)),
            dummy: hash(&token())?,
        })
    }

    pub(crate) async fn hash(&self, password: String) -> Result<String, ApiError> {
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

pub(crate) fn hash(password: &str) -> anyhow::Result<String> {
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
    pub(crate) async fn retry_after(&self, handle: &str) -> Option<u64> {
        let mut entries = self.0.lock().await;
        let current = now_ms();
        entries.retain(|_, (_, since)| current - *since < LOCK_MS);
        entries
            .get(handle)
            .filter(|(failures, _)| *failures >= MAX_FAILURES)
            .map(|(_, since)| {
                u64::try_from(LOCK_MS - (current - since))
                    .map_or(1, |left| left.div_ceil(MS_PER_SEC))
            })
    }

    pub(crate) async fn fail(&self, handle: &str) {
        let mut entries = self.0.lock().await;
        let entry = entries.entry(handle.to_owned()).or_insert((0, now_ms()));
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
    pub admin: bool,
    pub project: Option<String>,
    pub(crate) digest: String,
}

impl FromRequestParts<Shared> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, ApiError> {
        let secret = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|secret| {
                secret.len() == HEX_SECRET_LEN && secret.bytes().all(|b| b.is_ascii_hexdigit())
            })
            .ok_or(ApiError::Unauthorized)?;
        let digest = digest(secret);
        let row: Option<(String, Option<String>, Option<String>, bool)> = sqlx::query_as(
            "SELECT t.kind, t.handle, t.project_id, COALESCE(a.admin, 0) FROM tokens t
             LEFT JOIN accounts a ON a.handle = t.handle
             WHERE t.digest = ?1 AND (t.expires_at IS NULL OR t.expires_at > ?2)",
        )
        .bind(&digest)
        .bind(now_ms())
        .fetch_optional(&state.db)
        .await?;
        let (kind, handle, project, admin) = row.ok_or(ApiError::Unauthorized)?;
        let kind = match kind.as_str() {
            "session" => TokenKind::Session,
            "machine" => TokenKind::Machine,
            "device" => TokenKind::Device,
            _ => return Err(ApiError::Unauthorized),
        };
        sqlx::query("UPDATE tokens SET last_used_at = ?1 WHERE digest = ?2")
            .bind(now_ms())
            .bind(&digest)
            .execute(&state.db)
            .await?;
        let principal = Self {
            kind,
            handle,
            admin,
            project,
            digest,
        };
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
    let stored = if valid_handle(&handle) && form.password.len() <= *PASSWORD_LEN.end() {
        sqlx::query_scalar("SELECT password_hash FROM accounts WHERE handle = ?1")
            .bind(&handle)
            .fetch_optional(&state.db)
            .await?
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

pub(crate) async fn issue_session(state: &Shared, handle: &str) -> Result<SessionToken, ApiError> {
    let secret = token();
    let created_at = now_ms();
    let expires_at = created_at + SESSION_AGE_MS;
    sqlx::query("DELETE FROM tokens WHERE expires_at IS NOT NULL AND expires_at <= ?1")
        .bind(created_at)
        .execute(&state.db)
        .await?;
    sqlx::query(
        "INSERT INTO tokens (digest, kind, handle, label, created_at, expires_at)
         VALUES (?1, 'session', ?2, 'Session', ?3, ?4)",
    )
    .bind(digest(&secret))
    .bind(handle)
    .bind(created_at)
    .bind(expires_at)
    .execute(&state.db)
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
    sqlx::query("DELETE FROM tokens WHERE digest = ?1")
        .bind(&principal.digest)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/me`
pub async fn me(principal: Principal) -> Json<Me> {
    Json(Me {
        kind: principal.kind,
        handle: principal.handle,
        admin: principal.admin,
        project: principal.project,
    })
}

/// `GET /v1/setup`: whether the first administrator still has to be created.
pub async fn setup_status(State(state): State<Shared>) -> Result<Json<SetupStatus>, ApiError> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM accounts WHERE admin = 1 LIMIT 1")
        .fetch_optional(&state.db)
        .await?;
    Ok(Json(SetupStatus {
        required: exists.is_none(),
    }))
}

/// `POST /v1/setup`: create the first administrator and sign them in. Refused once
/// one exists, and without the setup code unless the backend only listens on
/// loopback.
pub async fn setup(
    State(state): State<Shared>,
    Json(form): Json<Setup>,
) -> Result<(StatusCode, Json<SessionToken>), ApiError> {
    let handle = form.handle.trim().to_ascii_lowercase();
    if let Some(retry_after) = state.lockout.retry_after(SETUP_LOCKOUT_KEY).await {
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
        state.lockout.fail(SETUP_LOCKOUT_KEY).await;
        return Err(ApiError::Forbidden);
    }
    if !valid_handle(&handle) || !PASSWORD_LEN.contains(&form.password.len()) {
        return Err(account_rules());
    }
    let password = state.passwords.hash(form.password).await?;
    let created = sqlx::query(
        "INSERT OR IGNORE INTO accounts (handle, password_hash, admin, created_at)
         SELECT ?1, ?2, 1, ?3 WHERE NOT EXISTS (SELECT 1 FROM accounts WHERE admin = 1)",
    )
    .bind(&handle)
    .bind(&password)
    .bind(now_ms())
    .execute(&state.db)
    .await?
    .rows_affected();
    if created == 0 {
        return Err(ApiError::Conflict("an administrator already exists"));
    }
    tracing::info!(handle, "created the first administrator account");
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
        assert_eq!(first.len(), HEX_SECRET_LEN);
        assert_ne!(first, second);
        assert_eq!(digest(&first).len(), HEX_SECRET_LEN);
        assert_ne!(digest(&first), first);
    }
}
