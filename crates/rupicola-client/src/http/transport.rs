use crate::{ClientError, Result};
use reqwest::{Method, RequestBuilder, header};
use rupicola_api::PREFIX;
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

/// Path of the MCP endpoint, which sits outside the versioned prefix.
pub(super) const MCP_PATH: &str = "/mcp";
pub(super) const OCTET_STREAM: &str = "application/octet-stream";
pub(super) const JSON: &str = "application/json";
const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
const HIGH_NIBBLE_SHIFT: u8 = 4;
const LOW_NIBBLE_MASK: u8 = 0x0F;

/// Longest standard API calls may take.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest blob uploads, artifact downloads, `run_capture`, and `mcp` calls may take.
pub(super) const LONG_TIMEOUT: Duration = Duration::from_secs(600);
/// Longest the backend gets to accept a connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Percent-encodes a single URL path segment per RFC 3986, also escaping `.` and `..`
/// dot-segments so they cannot traverse paths.
pub(super) fn encode_segment(segment: &str) -> String {
    if segment == "." {
        return "%2E".to_owned();
    }
    if segment == ".." {
        return "%2E%2E".to_owned();
    }
    let mut encoded = String::with_capacity(segment.len());
    for &byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(
                HEX_UPPER[usize::from(byte >> HIGH_NIBBLE_SHIFT)],
            ));
            encoded.push(char::from(HEX_UPPER[usize::from(byte & LOW_NIBBLE_MASK)]));
        }
    }
    encoded
}

/// The backend over HTTP.
#[derive(Clone, Debug)]
pub struct HttpApi {
    pub(super) base: String,
    pub(super) client: reqwest::Client,
}

impl HttpApi {
    /// A client for the backend at `server_url`, such as `https://rupicola.example`.
    ///
    /// # Errors
    ///
    /// Fails when the HTTP client cannot be built.
    pub fn new(server_url: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        Ok(Self {
            base: server_url.trim_end_matches('/').to_owned(),
            client,
        })
    }

    pub(super) fn request(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
    ) -> RequestBuilder {
        let builder = self
            .client
            .request(method, format!("{}{PREFIX}{path}", self.base));
        match token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    pub(super) async fn send(request: RequestBuilder) -> Result<reqwest::Response> {
        let response = request
            .send()
            .await
            .map_err(|e| ClientError::Unavailable(e.to_string()))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let retry_after = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        let body = response.text().await.unwrap_or_default();
        Err(ClientError::from_response(status, retry_after, &body))
    }

    pub(super) async fn read<T: DeserializeOwned>(request: RequestBuilder) -> Result<T> {
        Self::send(request)
            .await?
            .json()
            .await
            .map_err(|e| ClientError::Malformed(e.to_string()))
    }

    pub(super) async fn get_json<T: DeserializeOwned>(&self, token: &str, path: &str) -> Result<T> {
        Self::read(self.request(Method::GET, path, Some(token))).await
    }

    pub(super) async fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
        body: &B,
    ) -> Result<T> {
        Self::read(self.request(Method::POST, path, Some(token)).json(body)).await
    }

    pub(super) async fn put_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
        body: &B,
    ) -> Result<T> {
        Self::read(self.request(Method::PUT, path, Some(token)).json(body)).await
    }

    pub(super) async fn patch_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
        body: &B,
    ) -> Result<T> {
        Self::read(self.request(Method::PATCH, path, Some(token)).json(body)).await
    }

    pub(super) async fn delete_json<T: DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
    ) -> Result<T> {
        Self::read(self.request(Method::DELETE, path, Some(token))).await
    }

    pub(super) async fn delete_empty(&self, token: &str, path: &str) -> Result<()> {
        Self::send(self.request(Method::DELETE, path, Some(token))).await?;
        Ok(())
    }

    pub(super) async fn put_empty(&self, token: &str, path: &str) -> Result<()> {
        Self::send(self.request(Method::PUT, path, Some(token))).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::encode_segment;

    #[test]
    fn encode_segment_escapes_delimiters_and_dot_segments() {
        assert_eq!(encode_segment("demo-project_1.0~a"), "demo-project_1.0~a");
        assert_eq!(encode_segment(".."), "%2E%2E");
        assert_eq!(encode_segment("."), "%2E");
        assert_eq!(encode_segment("../etc/passwd"), "..%2Fetc%2Fpasswd");
        assert_eq!(encode_segment("a/b?c#d%e"), "a%2Fb%3Fc%23d%25e");
    }
}
