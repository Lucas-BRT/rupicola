use reqwest::StatusCode;
use rupicola_api::ErrorBody;

pub type Result<T> = std::result::Result<T, ClientError>;

/// Why a call to the backend failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("sign in or send a valid bearer token")]
    Unauthorized,
    #[error("this credential may not do that")]
    Forbidden,
    #[error("not found")]
    NotFound,
    /// The backend refused the request and says what to change.
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    #[error("too many requests{}", Self::retry_hint(*.retry_after))]
    RateLimited { retry_after: Option<u64> },
    /// The backend could not be reached, or failed on its own side.
    #[error("the backend is unavailable: {0}")]
    Unavailable(String),
    /// The backend answered with something this client cannot read.
    #[error("unreadable answer from the backend: {0}")]
    Malformed(String),
}

impl ClientError {
    fn retry_hint(retry_after: Option<u64>) -> String {
        retry_after.map_or_else(String::new, |seconds| format!("; retry in {seconds} s"))
    }

    /// The error for a non-success answer; `body` is the raw response body.
    pub(crate) fn from_response(status: StatusCode, retry_after: Option<u64>, body: &str) -> Self {
        let message = || {
            serde_json::from_str::<ErrorBody>(body)
                .map_or_else(|_| body.to_owned(), |error| error.message)
        };
        match status {
            StatusCode::UNAUTHORIZED => Self::Unauthorized,
            StatusCode::FORBIDDEN => Self::Forbidden,
            StatusCode::NOT_FOUND => Self::NotFound,
            StatusCode::CONFLICT => Self::Conflict(message()),
            StatusCode::TOO_MANY_REQUESTS => Self::RateLimited { retry_after },
            status if status.is_client_error() => Self::Invalid(message()),
            status => Self::Unavailable(format!("HTTP {status}")),
        }
    }
}
