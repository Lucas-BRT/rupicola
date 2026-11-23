//! How an [`ApiError`] is answered: a status, an [`ErrorBody`] and, when waiting
//! helps, a `Retry-After`.

use crate::domain::ApiError;
use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rupicola_api::ErrorBody;

/// Seconds an overloaded server asks clients to wait before retrying.
const OVERLOADED_RETRY_AFTER_SECS: u64 = 1;

fn status(error: &ApiError) -> StatusCode {
    match error {
        ApiError::NotFound => StatusCode::NOT_FOUND,
        ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
        ApiError::Forbidden => StatusCode::FORBIDDEN,
        ApiError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ApiError::Conflict(_) => StatusCode::CONFLICT,
        ApiError::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
        ApiError::Timeout => StatusCode::REQUEST_TIMEOUT,
        ApiError::Overloaded => StatusCode::SERVICE_UNAVAILABLE,
        ApiError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn code(error: &ApiError) -> &'static str {
    match error {
        ApiError::NotFound => "not-found",
        ApiError::Unauthorized => "unauthorized",
        ApiError::Forbidden => "forbidden",
        ApiError::Invalid(_) => "invalid",
        ApiError::Conflict(_) => "conflict",
        ApiError::TooManyRequests { .. } => "rate-limited",
        ApiError::Timeout => "timeout",
        ApiError::Overloaded => "overloaded",
        ApiError::PayloadTooLarge => "payload-too-large",
        ApiError::Internal(_) => "internal",
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let message = if let Self::Internal(error) = &self {
            tracing::error!(error = format!("{error:#}"), "request failed");
            "internal server error".to_owned()
        } else {
            self.to_string()
        };
        let retry_after = match &self {
            Self::TooManyRequests { retry_after } => Some(retry_after.to_string()),
            Self::Overloaded => Some(OVERLOADED_RETRY_AFTER_SECS.to_string()),
            _ => None,
        };
        let body = ErrorBody {
            code: code(&self).to_owned(),
            message,
        };
        let mut response = (status(&self), Json(body)).into_response();
        if let Some(value) = retry_after.and_then(|value| HeaderValue::from_str(&value).ok()) {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_hide_their_details_from_clients() {
        let response = ApiError::from(anyhow::anyhow!("database path /secret")).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn rate_limited_responses_say_when_to_retry() {
        const WAIT_SECS: u64 = 7;
        let response = ApiError::TooManyRequests {
            retry_after: WAIT_SECS,
        }
        .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers()[header::RETRY_AFTER],
            WAIT_SECS.to_string()
        );
    }
}
