//! Secrets, their digests, and the clock every record is stamped with.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

/// Random bytes in every secret (sessions, tokens, invitation codes).
const SECRET_BYTES: usize = 32;
/// Length of a secret or SHA-256 digest written as hex.
pub const HEX_SECRET_LEN: usize = 64;

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

/// Whether `secret` has the shape of one this server issues.
pub fn well_formed(secret: &str) -> bool {
    secret.len() == HEX_SECRET_LEN && secret.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_hex_and_stored_only_as_digests() {
        let (first, second) = (token(), token());
        assert_eq!(first.len(), HEX_SECRET_LEN);
        assert_ne!(first, second);
        assert_eq!(digest(&first).len(), HEX_SECRET_LEN);
        assert_ne!(digest(&first), first);
    }
}
