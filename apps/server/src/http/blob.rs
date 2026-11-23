//! Answering a request with a stored blob.

use crate::{
    domain::ApiError,
    storage::blobs::{Blobs, Stored},
};
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
    blobs: &Blobs,
    sha256: &str,
    mime: &str,
    request: Request,
) -> Result<Response, ApiError> {
    let mime = HeaderValue::from_str(mime).map_err(anyhow::Error::from)?;
    let mut response = match blobs.locate(sha256)? {
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
