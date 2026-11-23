//! Password hashing and the per-account lockout that slows guessing.

use crate::domain::{ApiError, secret};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// Failed sign-ins per account before it is locked for [`LOCK_MS`].
const MAX_FAILURES: u32 = 8;
/// How long a locked account stays locked, in milliseconds (5 minutes).
const LOCK_MS: i64 = 5 * 60 * 1000;
/// Milliseconds in a second, to round lock waits up to whole seconds.
const MS_PER_SEC: u64 = 1000;
/// Argon2 hashes computed at once; more requests get 503 instead of piling up.
const PASSWORD_WORKERS: usize = 4;

/// Argon2 hashing off the async threads, with bounded parallelism. Unknown accounts
/// are verified against a dummy hash so timing does not reveal which handles exist.
pub struct Passwords {
    work: Arc<Semaphore>,
    dummy: String,
}

impl Passwords {
    /// # Errors
    ///
    /// Returns an error if the dummy hash cannot be computed.
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            work: Arc::new(Semaphore::new(PASSWORD_WORKERS)),
            dummy: hash(&secret::token())?,
        })
    }

    pub async fn hash(&self, password: String) -> Result<String, ApiError> {
        let permit = self.permit()?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hash(&password)
        })
        .await
        .map_err(anyhow::Error::from)?
        .map_err(ApiError::Internal)
    }

    /// Whether `password` matches `stored`; always `false` without a stored hash, after
    /// the same work as a real check.
    pub async fn verify(&self, stored: Option<String>, password: String) -> Result<bool, ApiError> {
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

    fn permit(&self) -> Result<OwnedSemaphorePermit, ApiError> {
        self.work
            .clone()
            .try_acquire_owned()
            .map_err(|_| ApiError::Overloaded)
    }
}

/// An Argon2 hash of `password`, computed on the current thread.
pub fn hash(password: &str) -> anyhow::Result<String> {
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
    pub async fn retry_after(&self, handle: &str) -> Option<u64> {
        let mut entries = self.0.lock().await;
        let current = secret::now_ms();
        entries.retain(|_, (_, since)| current - *since < LOCK_MS);
        entries
            .get(handle)
            .filter(|(failures, _)| *failures >= MAX_FAILURES)
            .map(|(_, since)| {
                u64::try_from(LOCK_MS - (current - since))
                    .map_or(1, |left| left.div_ceil(MS_PER_SEC))
            })
    }

    /// Fails with [`ApiError::TooManyRequests`] while `handle` is locked.
    pub async fn check(&self, handle: &str) -> Result<(), ApiError> {
        match self.retry_after(handle).await {
            Some(retry_after) => Err(ApiError::TooManyRequests { retry_after }),
            None => Ok(()),
        }
    }

    pub async fn fail(&self, handle: &str) {
        let mut entries = self.0.lock().await;
        let entry = entries
            .entry(handle.to_owned())
            .or_insert((0, secret::now_ms()));
        entry.0 = entry.0.saturating_add(1);
    }

    pub async fn clear(&self, handle: &str) {
        self.0.lock().await.remove(handle);
    }
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
}
