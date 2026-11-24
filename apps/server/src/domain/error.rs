//! The ways a request can fail. The HTTP layer turns each into a status and an
//! [`rupicola_api::ErrorBody`].

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Also used when the caller may not see the resource, so its existence
    /// does not leak.
    #[error("not found")]
    NotFound,
    #[error("sign in or send a valid bearer token")]
    Unauthorized,
    #[error("this credential may not do that")]
    Forbidden,
    #[error("{0}")]
    Invalid(String),
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

impl ApiError {
    /// A 422 explaining what the client must change.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}
