//! Errors returned by API handlers and middleware, rendered as [`ErrorBody`] JSON.

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rupicola_api::ErrorBody;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("no route matches this request")]
    NotFound,
    #[error("sign in or send a valid bearer token")]
    Unauthorized,
    #[error("this credential may not do that")]
    Forbidden,
    #[error("{0}")]
    Invalid(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error("too many requests; retry in {retry_after} s")]
    TooManyRequests { retry_after: u64 },
    #[error("the request took too long")]
    Timeout,
    #[error("the server is busy; retry shortly")]
    Overloaded,
    #[error("the request body is too large")]
    PayloadTooLarge,
    /// Unexpected failure; details go to the log, never to the client.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<turso::Error> for ApiError {
    fn from(error: turso::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Timeout => StatusCode::REQUEST_TIMEOUT,
            Self::Overloaded => StatusCode::SERVICE_UNAVAILABLE,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not-found",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::Invalid(_) => "invalid",
            Self::Conflict(_) => "conflict",
            Self::TooManyRequests { .. } => "rate-limited",
            Self::Timeout => "timeout",
            Self::Overloaded => "overloaded",
            Self::PayloadTooLarge => "payload-too-large",
            Self::Internal(_) => "internal",
        }
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
            Self::Overloaded => Some("1".to_owned()),
            _ => None,
        };
        let body = ErrorBody {
            code: self.code().to_owned(),
            message,
        };
        let mut response = (self.status(), Json(body)).into_response();
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
        let response = ApiError::TooManyRequests { retry_after: 7 }.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "7");
    }
}
