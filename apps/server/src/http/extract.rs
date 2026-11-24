//! Request extractors: the bearer token becomes the [`Principal`] behind it.

use crate::{
    domain::{ApiError, Principal},
    services,
    state::Shared,
};
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};

/// What precedes the secret in an `Authorization` header.
const BEARER_PREFIX: &str = "Bearer ";

impl FromRequestParts<Shared> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, ApiError> {
        let secret = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix(BEARER_PREFIX))
            .ok_or(ApiError::Unauthorized)?;
        services::auth::principal(state, secret).await
    }
}
