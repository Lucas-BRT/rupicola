//! Answering a request with a stored blob.

use crate::{domain::ApiError, state::Shared, storage::blobs::Stored};
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, header},
    response::Response,
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

/// Answers `request` with the blob's bytes as `mime`. On disk this supports range requests
/// (video seeking) and conditional requests.
///
/// # Errors
///
/// [`ApiError::NotFound`] when the blob is missing.
pub async fn respond(
    state: &Shared,
    sha256: String,
    mime: &str,
    request: Request,
) -> Result<Response, ApiError> {
    let mime = HeaderValue::from_str(mime).map_err(anyhow::Error::from)?;
    let shared = state.clone();
    let located = tokio::task::spawn_blocking(move || shared.blobs.locate(&sha256))
        .await
        .map_err(anyhow::Error::from)??;
    let mut response = match located {
        Stored::File(path) => ServeFile::new(path)
            .oneshot(request)
            .await
            .map_err(anyhow::Error::from)?
            .map(Body::new),
        #[cfg(test)]
        Stored::Bytes(data) => {
            drop(request);
            axum::response::IntoResponse::into_response(data)
        }
    };
    // Blobs carry no extension, so the recorded media type replaces any guess.
    response.headers_mut().insert(header::CONTENT_TYPE, mime);
    Ok(response)
}
