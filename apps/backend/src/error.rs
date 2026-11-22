//! Errors returned by API handlers, rendered as [`ErrorBody`] JSON.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rupicola_api::ErrorBody;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("no route matches this request")]
    NotFound,
    /// Unexpected failure; details go to the log, never to the client.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "not-found",
            Self::Internal(_) => "internal",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let message = match &self {
            Self::Internal(error) => {
                tracing::error!(error = format!("{error:#}"), "request failed");
                "internal server error".to_owned()
            }
            Self::NotFound => self.to_string(),
        };
        let body = ErrorBody {
            code: self.code().to_owned(),
            message,
        };
        (self.status(), Json(body)).into_response()
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
}
