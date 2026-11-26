//! Sprint 2 (P1) tests:
//! 1. Limits enforcement (`max_blob_bytes`, `MAX_READ_BYTES` clamp, HTTP 413 body limit).
//! 2. Graceful degradation on corrupt media inputs (structured MCP error, zero panics).
//! 3. SQLite migrations idempotency, foreign-key integrity, and `PRAGMA integrity_check`.

use super::tests::{payload, rpc, tool};
use crate::testkit::*;
use axum::http::StatusCode;
use base64::Engine as _;
use serde_json::json;

#[tokio::test]
async fn limits_enforce_max_blob_bytes_and_body_size_and_artifact_read_clamp() {
    let (port, [ana, ..]) = demo_with_artifacts().await;

    // 1. Tighten project `max_blob_bytes` to 64 bytes and verify capture_publish rejects larger blobs
    let tighten = call(
        port,
        "PUT",
        "/v1/projects/demo/settings/max_blob_bytes",
        Some(&ana),
        Some(r#"{"value":64}"#),
    )
    .await;
    assert_eq!(status(&tighten), StatusCode::OK, "{tighten}");

    let big_b64 = base64::engine::general_purpose::STANDARD.encode(vec![b'x'; 128]);
    let rejected = rpc(
        port,
        &ana,
        &tool(
            "capture_publish",
            &json!({
                "project": "demo",
                "scenario": "too-big",
                "members": [{"id": "big", "data_base64": big_b64}]
            }),
        ),
    )
    .await;
    assert_eq!(
        rejected["result"]["isError"], true,
        "blob exceeding max_blob_bytes must fail: {rejected}"
    );

    // 2. artifact_read clamps excessive length safely without error or OOM
    let small_b64 = base64::engine::general_purpose::STANDARD.encode(b"tiny-payload");
    let cap = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({
                    "project": "demo",
                    "scenario": "tiny",
                    "members": [{"id": "small", "mime": "application/octet-stream", "data_base64": small_b64}]
                }),
            ),
        )
        .await,
    );
    let art_id = cap["checkpoints"][0]["artifact"]["id"].as_str().unwrap();
    let chunk = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "artifact_read",
                &json!({"id": art_id, "offset": 0, "length": 1_048_576}),
            ),
        )
        .await,
    );
    assert_eq!(chunk["length"], 12);

    // 3. Oversized HTTP body on POST /mcp returns 413 Payload Too Large
    let mut small_sec = settings();
    small_sec.body_limit = 256;
    let (small_port, state) = start_custom(small_sec, None, true).await;
    seed_account(&state, "alice", false).await;
    let alice = sign_in(small_port, "alice").await;
    let huge = "a".repeat(1024);
    let resp = call(
        small_port,
        "POST",
        "/mcp",
        Some(&alice),
        Some(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"ping","pad":"{huge}"}}"#
        )),
    )
    .await;
    assert_eq!(status(&resp), StatusCode::PAYLOAD_TOO_LARGE, "{resp}");
}

#[tokio::test]
async fn corrupt_media_degrades_gracefully_with_structured_mcp_errors() {
    let (port, [ana, ..]) = demo_with_artifacts().await;
    let garbage_b64 = base64::engine::general_purpose::STANDARD.encode(b"not-a-valid-media-file");
    let cap = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({
                    "project": "demo",
                    "scenario": "corrupt",
                    "members": [
                        {"id": "bad-png", "role": "checkpoint", "mime": "image/png", "data_base64": garbage_b64},
                        {"id": "bad-mp4", "role": "clip-video", "mime": "video/mp4", "data_base64": garbage_b64}
                    ]
                }),
            ),
        )
        .await,
    );
    let bad_png = cap["checkpoints"][0]["artifact"]["id"].as_str().unwrap();
    let bad_mp4 = cap["clips"][0]["video"]["id"].as_str().unwrap();

    for req in [
        tool("media_probe", &json!({"artifact_id": bad_png})),
        tool("media_probe", &json!({"artifact_id": bad_mp4})),
        tool("media_motion_diff", &json!({"artifact_id": bad_mp4})),
        tool(
            "media_contact_sheet",
            &json!({"artifact_id": bad_mp4, "cols": 2, "rows": 2}),
        ),
        tool(
            "visual_crop_region",
            &json!({"artifact_id": bad_png, "x": 0, "y": 0, "width": 4, "height": 4}),
        ),
        tool(
            "visual_diff_images",
            &json!({"before_artifact_id": bad_png, "after_artifact_id": bad_png}),
        ),
    ] {
        let reply = rpc(port, &ana, &req).await;
        assert_eq!(
            reply["result"]["isError"], true,
            "corrupt media should return structured MCP error, not crash: {reply}"
        );
    }
}

#[tokio::test]
async fn sqlite_migrations_are_idempotent_and_pass_integrity_checks() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // Re-running migrations on an already-migrated DB must be a clean no-op.
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let fk_errors: Vec<(String,)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(fk_errors.len(), 0);

    let (integrity,): (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
}
