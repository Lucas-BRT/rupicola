//! Client-side interface traits: credential storage, MCP transport, and reusable
//! binary blob deduplication helpers.

use crate::{McpReply, Result};
use async_trait::async_trait;
use rupicola_api::{BlobTransferOps, MissingQuery};
use std::collections::BTreeSet;

/// Stored server address and bearer token for a Rupicola client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSession {
    pub server_url: Option<String>,
    pub token: Option<String>,
}

/// Pluggable credential persistence (filesystem store, OS keyring, CI env, or in-memory mock).
pub trait CredentialStore: Send + Sync {
    /// Reads the stored server URL and bearer token.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot be read.
    fn load(&self) -> Result<StoredSession>;

    /// Persists the server URL and/or bearer token.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot be written.
    fn save(&self, server_url: Option<&str>, token: Option<&str>) -> Result<()>;

    /// Removes stored credentials.
    ///
    /// # Errors
    ///
    /// Returns an error when the backing store cannot be cleared.
    fn clear(&self) -> Result<()>;
}

/// Transport for relaying raw MCP JSON-RPC payloads (HTTP backend, in-process server, or mock).
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&self, payload: &str) -> Result<McpReply>;
}

/// Uploads only the blobs missing on the backend for `project`, deduplicating by SHA-256.
///
/// # Errors
///
/// Propagates any backend failure from `missing_blobs` or `upload_project_blob`.
pub async fn upload_missing_blobs<T>(
    client: &T,
    project: &str,
    blobs: Vec<(String, Vec<u8>)>,
) -> std::result::Result<usize, T::Error>
where
    T: BlobTransferOps + ?Sized,
{
    if blobs.is_empty() {
        return Ok(0);
    }
    let hashes: Vec<String> = blobs
        .iter()
        .map(|(sha, _)| sha.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let missing = client
        .missing_blobs(project, &MissingQuery { sha256: hashes })
        .await?;
    let needed: BTreeSet<String> = missing.missing.into_iter().collect();
    let mut uploaded = BTreeSet::new();
    for (sha, data) in blobs {
        if needed.contains(&sha) && uploaded.insert(sha.clone()) {
            client.upload_project_blob(project, &sha, data).await?;
        }
    }
    Ok(uploaded.len())
}

#[cfg(test)]
mod tests {
    use super::upload_missing_blobs;
    use crate::{MockApi, WorkspaceClient};
    use rupicola_api::Missing;

    #[tokio::test]
    async fn upload_missing_blobs_deduplicates_and_skips_present() {
        let mut mock = MockApi::new();
        mock.expect_missing_blobs()
            .times(1)
            .withf(|tok, proj, q| tok == "tok" && proj == "demo" && q.sha256 == vec!["aaa", "bbb"])
            .returning(|_, _, _| {
                Ok(Missing {
                    missing: vec!["bbb".to_owned()],
                    limits: rupicola_api::Limits {
                        max_blob_bytes: 1024,
                        max_image_bytes: 1024,
                        max_video_bytes: 1024,
                        max_artifacts_per_capture: 10,
                        max_body_bytes: 1024,
                        max_missing_batch: 10,
                        upload_timeout_secs: 60,
                        upload_idle_secs: 30,
                    },
                })
            });
        mock.expect_upload_project_blob()
            .times(1)
            .withf(|tok, proj, sha, data| {
                tok == "tok" && proj == "demo" && sha == "bbb" && data == b"two"
            })
            .returning(|_, _, _, _| Ok(()));

        let client = WorkspaceClient::new(mock, "tok");
        let count = upload_missing_blobs(
            &client,
            "demo",
            vec![
                ("aaa".to_owned(), b"one".to_vec()),
                ("bbb".to_owned(), b"two".to_vec()),
                ("bbb".to_owned(), b"two".to_vec()),
            ],
        )
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn upload_missing_blobs_noop_when_empty() {
        let client = WorkspaceClient::new(MockApi::new(), "tok");
        let count = upload_missing_blobs(&client, "demo", Vec::new())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
