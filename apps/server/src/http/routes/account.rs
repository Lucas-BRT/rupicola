//! The signed-in account: `/me`, `/sessions`, and the routes that create or open one.

use super::super::{
    handlers::{account as auth, credentials},
    security,
};
use crate::state::Shared;
use axum::{Router, routing};

/// Password guessing budget per client: a burst of attempts, then one more every
/// [`SIGN_IN_REFILL_MS`].
pub const SIGN_IN_BURST: u32 = 5;
const SIGN_IN_REFILL_MS: u64 = 12_000;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/setup", routing::get(auth::setup_status))
        .nest(
            "/me",
            Router::new()
                .route("/", routing::get(auth::me))
                .route("/password", routing::put(auth::change_password)),
        )
        .nest(
            "/sessions",
            Router::new()
                .route("/current", routing::delete(auth::sign_out))
                .route("/others", routing::delete(auth::sign_out_others)),
        )
}

/// The routes that take a password or a one-time code, which get a much smaller budget
/// than ordinary API use.
///
/// # Errors
///
/// Returns an error if the rate-limit configuration is invalid.
pub fn sign_in(settings: &security::Security) -> anyhow::Result<Router<Shared>> {
    let limiter = security::rate_limit(SIGN_IN_BURST, SIGN_IN_REFILL_MS, settings.behind_proxy)?;
    Ok(Router::new()
        .route("/sessions", routing::post(auth::sign_in))
        .route("/setup", routing::post(auth::setup))
        .route("/register", routing::post(credentials::register))
        .route("/invites/redeem", routing::post(credentials::redeem))
        .route_layer(limiter))
}
