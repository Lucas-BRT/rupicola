use super::WorkspaceClient;
use crate::{ArtifactBody, ClientError, HttpApi, McpReply, McpTransport, MockApi};
use base64::Engine as _;
use rupicola_api::{
    AdminAuthOps, ArtifactReadOps, AuditorOps, CliOps, Credential, DeletionPolicy, FullAgentOps,
    IssuedCredential, LimitSetting, LimitSettings, LimitValue, Limits, MediaInspectOps,
    NewCredential, OfflineCatalogOps, PanelOps, PolicyView, ProjectAdminOps, RunnerOps, TokenKind,
    WorkspaceOps,
};

#[test]
fn workspace_client_satisfies_every_composed_supertrait() {
    fn assert_all<
        T: WorkspaceOps
            + AuditorOps
            + FullAgentOps
            + RunnerOps
            + AdminAuthOps
            + CliOps
            + PanelOps
            + McpTransport,
    >() {
    }
    assert_all::<WorkspaceClient<HttpApi>>();
}

#[tokio::test]
async fn artifact_read_parses_content_range_and_encodes_base64() {
    let mut mock = MockApi::new();
    mock.expect_artifact()
        .times(1)
        .withf(|tok, id, range| {
            tok == "tok" && id == "art-1" && range.as_deref() == Some("bytes=6-10")
        })
        .returning(|_, _, _| {
            Ok(ArtifactBody {
                status: 206,
                content_type: Some("text/plain".to_owned()),
                content_range: Some("bytes 6-10/11".to_owned()),
                bytes: b"world".to_vec(),
            })
        });

    let client = WorkspaceClient::new(mock, "tok");
    let chunk = client.artifact_read("art-1", 6, 5).await.unwrap();
    assert_eq!(chunk.bytes, 11);
    assert_eq!(chunk.length, 5);
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(&chunk.data)
            .unwrap(),
        b"world"
    );
}

#[tokio::test]
async fn mcp_tool_error_maps_to_client_invalid() {
    let mut mock = MockApi::new();
    mock.expect_mcp().times(1).returning(|_, _| {
        Ok(McpReply {
            status: 200,
            body: serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "content": [{"type": "text", "text": "ffprobe unavailable"}],
                    "isError": true
                }
            })
            .to_string(),
        })
    });

    let client = WorkspaceClient::new(mock, "tok");
    let err = client.media_probe("art-9").await.unwrap_err();
    assert!(matches!(err, ClientError::Invalid(ref m) if m.contains("ffprobe")));
}

#[tokio::test]
async fn offline_catalog_ops_run_locally_without_network() {
    let client = WorkspaceClient::new(MockApi::new(), "tok");
    let rules = client.offline_rules().await.unwrap();
    let modules = client.offline_modules().await.unwrap();
    assert_ne!(rules.as_array().unwrap().len(), 0);
    assert_ne!(modules.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn project_admin_device_ops_forward_requests() {
    let mut mock = MockApi::new();
    mock.expect_list_devices()
        .times(1)
        .withf(|tok, proj| tok == "tok" && proj == "demo")
        .returning(|_, _| {
            Ok(vec![Credential {
                id: "dev-1".to_owned(),
                kind: TokenKind::Device,
                project_id: Some("demo".to_owned()),
                label: "ci-runner".to_owned(),
                created_at: 10,
                last_used_at: None,
                expires_at: None,
            }])
        });
    mock.expect_create_device()
        .times(1)
        .withf(|tok, proj, form| tok == "tok" && proj == "demo" && form.label == "mac-mini")
        .returning(|_, _, form| {
            Ok(IssuedCredential {
                credential: Credential {
                    id: "dev-2".to_owned(),
                    kind: TokenKind::Device,
                    project_id: Some("demo".to_owned()),
                    label: form.label.clone(),
                    created_at: 20,
                    last_used_at: None,
                    expires_at: None,
                },
                token: "rpk_dev_secret".to_owned(),
            })
        });
    mock.expect_delete_device()
        .times(1)
        .withf(|tok, proj, dev| tok == "tok" && proj == "demo" && dev == "dev-1")
        .returning(|_, _, _| Ok(()));

    let client = WorkspaceClient::new(mock, "tok");
    assert_eq!(client.device_list("demo").await.unwrap().len(), 1);
    let issued = client
        .device_create(
            "demo",
            &NewCredential {
                label: "mac-mini".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(issued.token, "rpk_dev_secret");
    client.device_delete("demo", "dev-1").await.unwrap();
}

#[tokio::test]
async fn project_admin_policy_and_limit_ops_forward_requests() {
    let limits = |blob| Limits {
        max_blob_bytes: blob,
        max_image_bytes: 512,
        max_video_bytes: 2048,
        max_artifacts_per_capture: 50,
        max_body_bytes: 65536,
        max_missing_batch: 1000,
        upload_timeout_secs: 60,
        upload_idle_secs: 30,
    };
    let mut mock = MockApi::new();
    mock.expect_deletion_policy().times(1).returning(|_, _| {
        Ok(PolicyView {
            policy: DeletionPolicy::Confirm,
            server_allows: true,
        })
    });
    mock.expect_set_deletion_policy()
        .times(1)
        .withf(|_, proj, form| proj == "demo" && form.policy == DeletionPolicy::Forbid)
        .returning(|_, _, form| Ok(form.clone()));
    mock.expect_project_settings()
        .times(1)
        .returning(move |_, _| {
            Ok(LimitSettings {
                effective: limits(1024),
                set: std::collections::BTreeMap::new(),
            })
        });
    mock.expect_set_project_setting()
        .times(1)
        .withf(|_, proj, key, val| proj == "demo" && key == "max_blob_bytes" && val.value == 4096)
        .returning(move |_, _, _, _| {
            Ok(LimitSettings {
                effective: limits(4096),
                set: std::collections::BTreeMap::from([(LimitSetting::MaxBlobBytes, 4096)]),
            })
        });
    mock.expect_reset_project_setting()
        .times(1)
        .withf(|_, proj, key| proj == "demo" && key == "max_blob_bytes")
        .returning(move |_, _, _| {
            Ok(LimitSettings {
                effective: limits(1024),
                set: std::collections::BTreeMap::new(),
            })
        });

    let client = WorkspaceClient::new(mock, "tok");
    assert_eq!(
        client.deletion_policy_get("demo").await.unwrap().policy,
        DeletionPolicy::Confirm
    );
    let updated = client
        .deletion_policy_set(
            "demo",
            &PolicyView {
                policy: DeletionPolicy::Forbid,
                server_allows: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.policy, DeletionPolicy::Forbid);
    assert_eq!(
        client
            .project_settings_get("demo")
            .await
            .unwrap()
            .effective
            .max_blob_bytes,
        1024
    );
    let set_res = client
        .project_settings_set("demo", "max_blob_bytes", &LimitValue { value: 4096 })
        .await
        .unwrap();
    assert_eq!(set_res.effective.max_blob_bytes, 4096);
    let reset_res = client
        .project_settings_reset("demo", "max_blob_bytes")
        .await
        .unwrap();
    assert!(reset_res.set.is_empty());
}
