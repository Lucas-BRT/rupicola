use super::Account;
use crate::{PageResult, Panel, StatusCode};
use http::{HeaderMap, HeaderValue, header};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Cookie that holds the backend's session token.
pub(super) const SESSION_COOKIE: &str = "rp_session";
/// Cookie that ties a sign-in or registration form to the browser that loaded it.
pub(super) const FORM_COOKIE: &str = "rp_form";
/// How long a loaded sign-in form stays valid, in seconds.
pub(super) const FORM_AGE: i64 = 1800;
/// Mixed into a session token to derive the CSRF token of its forms.
const CSRF_SEED: &str = "rupicola-csrf:";

pub(super) fn token() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system provides random bytes");
    let mut token = String::with_capacity(2 * bytes.len());
    for byte in bytes {
        token.push(char::from(HEX[usize::from(byte >> 4)]));
        token.push(char::from(HEX[usize::from(byte & 0xf)]));
    }
    token
}

pub(super) fn digest(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

pub(super) fn now() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    i64::try_from(secs).unwrap_or(i64::MAX)
}

/// The CSRF token of the forms of `session`: stable for the session and unguessable
/// without it, so the panel keeps no state.
pub(super) fn csrf_for(session: &str) -> String {
    digest(&format!("{CSRF_SEED}{session}"))
}

pub(super) fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let (_, value) = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|value| value.trim().split_once('='))
        .find(|(key, _)| *key == name)?;
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_graphic())).then(|| value.to_owned())
}

pub(super) fn cookie_header(panel: &Panel, name: &str, value: &str, age: i64) -> HeaderValue {
    let secure = if panel.secure { "; Secure" } else { "" };
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}{secure}")
        .parse()
        .unwrap()
}

pub fn same_origin(panel: &Panel, headers: &HeaderMap) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|s| s.to_str().ok())
        .is_some_and(|origin| {
            origin == panel.origin || (!panel.secure && origin == panel.localhost_origin)
        })
}

pub fn equal(a: &str, b: &str) -> bool {
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

pub fn check_csrf(account: &Account, csrf: &str) -> PageResult<()> {
    if equal(&account.csrf, csrf) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}
