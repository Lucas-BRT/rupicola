use super::{Call, TOOLS};
use crate::testkit::*;
use axum::http::StatusCode;
use base64::Engine as _;
use serde_json::{Value, json};

/// Calls `/mcp` as `token` and returns the JSON-RPC reply.
pub(super) async fn rpc(port: u16, token: &str, body: &Value) -> Value {
    let response = call(port, "POST", "/mcp", Some(token), Some(&body.to_string())).await;
    assert_eq!(status(&response), StatusCode::OK, "{response}");
    json(&response)
}

pub(super) fn tool(name: &str, arguments: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": name, "arguments": arguments}})
}

/// The JSON a successful tool call returned.
pub(super) fn payload(reply: &Value) -> Value {
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn every_catalog_entry_is_a_call_variant() {
    for tool in TOOLS {
        let outcome = serde_json::from_value::<Call>(json!({"name": tool.name, "arguments": {}}));
        if let Err(error) = outcome {
            assert!(
                !error.to_string().contains("unknown variant"),
                "{} has no Call variant: {error}",
                tool.name
            );
        }
    }
}

#[test]
fn catalog_matches_workspace_ops_exactly() {
    fn assert_server_scopes<
        T: rupicola_api::WorkspaceOps + rupicola_api::AuditorOps + rupicola_api::FullAgentOps,
    >() {
    }
    assert_server_scopes::<crate::http::ops::ServerOps>();
    let catalog_names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
    assert_eq!(catalog_names, rupicola_api::WORKSPACE_OP_NAMES);
}

#[tokio::test]
async fn extended_workspace_ops_work_through_mcp() {
    let (port, [ana, ..]) = demo_with_artifacts().await;
    let me = payload(&rpc(port, &ana, &tool("me", &json!({}))).await);
    assert_eq!(me["handle"], "ana");

    let limits = payload(
        &rpc(
            port,
            &ana,
            &tool("project_limits", &json!({"project": "demo"})),
        )
        .await,
    );
    assert!(limits["max_blob_bytes"].as_u64().unwrap() > 0);

    let topic = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "topic_create",
                &json!({"project": "demo", "path": "auth/login"}),
            ),
        )
        .await,
    );
    assert_eq!(topic["name"], "login");
    let tree = payload(&rpc(port, &ana, &tool("topic_tree", &json!({"project": "demo"}))).await);
    assert_ne!(tree["topics"].as_array().unwrap().len(), 0);

    let ds = payload(
        &rpc(
            port,
            &ana,
            &tool("design_system_create", &json!({"project": "demo"})),
        )
        .await,
    );
    assert!(ds["id"].is_string());

    let rules = payload(&rpc(port, &ana, &tool("offline_rules", &json!({}))).await);
    assert_ne!(rules.as_array().unwrap().len(), 0);
    let modules = payload(&rpc(port, &ana, &tool("offline_modules", &json!({}))).await);
    assert_ne!(modules.as_array().unwrap().len(), 0);
}

pub(super) fn make_solid_png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let data: Vec<u8> = std::iter::repeat_n(rgba, (width * height) as usize)
            .flatten()
            .collect();
        w.write_image_data(&data).unwrap();
    }
    out
}

#[tokio::test]
async fn visual_diff_crop_and_media_probe_work_through_mcp() {
    let (port, [ana, ..]) = demo_with_artifacts().await;
    let a_b64 =
        base64::engine::general_purpose::STANDARD.encode(make_solid_png(16, 16, [20, 20, 20, 255]));
    let b_b64 = base64::engine::general_purpose::STANDARD.encode(make_solid_png(
        16,
        16,
        [220, 40, 20, 255],
    ));

    let cap_a = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({
                    "project": "demo",
                    "scenario": "diff-a",
                    "members": [{"id": "a", "data_base64": a_b64}]
                }),
            ),
        )
        .await,
    );
    let cap_b = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({
                    "project": "demo",
                    "scenario": "diff-b",
                    "members": [{"id": "b", "data_base64": b_b64}]
                }),
            ),
        )
        .await,
    );
    let art_a = cap_a["checkpoints"][0]["artifact"]["id"].as_str().unwrap();
    let art_b = cap_b["checkpoints"][0]["artifact"]["id"].as_str().unwrap();

    let probe = payload(
        &rpc(
            port,
            &ana,
            &tool("media_probe", &json!({"artifact_id": art_a})),
        )
        .await,
    );
    assert_eq!(
        (probe["width"].as_u64(), probe["height"].as_u64()),
        (Some(16), Some(16))
    );

    let diff = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "visual_diff_images",
                &json!({"before_artifact_id": art_a, "after_artifact_id": art_b, "mode": "highlight"}),
            ),
        )
        .await,
    );
    assert!(diff["changed_ratio"].as_f64().unwrap() > 0.99);
    assert_ne!(diff["changed_regions"].as_array().unwrap().len(), 0);
    let diff_id = diff["diff_artifact_id"].as_str().unwrap();
    let diff_probe = payload(
        &rpc(
            port,
            &ana,
            &tool("media_probe", &json!({"artifact_id": diff_id})),
        )
        .await,
    );
    assert_eq!(diff_probe["width"], 16);

    let crop = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "visual_crop_region",
                &json!({"artifact_id": art_b, "x": 2, "y": 2, "width": 8, "height": 8}),
            ),
        )
        .await,
    );
    assert_eq!(crop["mime"], "image/png");
    assert!(crop["length"].as_u64().unwrap() > 0);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn progressive_video_inspection_pipeline_end_to_end() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        return;
    }
    let (port, [ana, ..]) = demo_with_artifacts().await;
    let dir = tempfile::tempdir().unwrap();
    let mp4_path = dir.path().join("flash.mp4");
    // 0.3s black + 0.1s white flash + 0.3s black at 30fps (21 frames total)
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:d=0.3:r=30",
            "-f",
            "lavfi",
            "-i",
            "color=c=white:s=64x64:d=0.1:r=30",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:d=0.3:r=30",
            "-filter_complex",
            "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]",
            "-map",
            "[v]",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&mp4_path)
        .status()
        .unwrap();
    assert!(status.success());

    let mp4_b64 =
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(&mp4_path).unwrap());
    let cap = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({
                    "project": "demo",
                    "scenario": "flash-clip",
                    "members": [{"id": "clip", "role": "clip-video", "mime": "video/mp4", "data_base64": mp4_b64}]
                }),
            ),
        )
        .await,
    );
    let clip_art = cap["clips"][0]["video"]["id"].as_str().unwrap();

    // 1. Probe metadata (0 images in context)
    let probe = payload(
        &rpc(
            port,
            &ana,
            &tool("media_probe", &json!({"artifact_id": clip_art})),
        )
        .await,
    );
    assert_eq!(
        (probe["width"].as_u64(), probe["height"].as_u64()),
        (Some(64), Some(64))
    );
    assert!(probe["frame_count"].as_u64().unwrap() >= 15);

    // 2. Motion diff detects the white flash numerically in pure JSON
    let motion = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "media_motion_diff",
                &json!({"artifact_id": clip_art, "threshold": 0.2}),
            ),
        )
        .await,
    );
    let spikes = motion["spikes"].as_array().unwrap();
    assert_ne!(spikes.len(), 0);
    assert!(
        spikes
            .iter()
            .any(|s| s["luma_delta"].as_f64().unwrap_or(0.0).abs() > 0.5),
        "expected strong luma flash spike in {motion}"
    );

    // 3. Contact sheet renders 1 PNG grid
    let sheet = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "media_contact_sheet",
                &json!({"artifact_id": clip_art, "cols": 3, "rows": 2}),
            ),
        )
        .await,
    );
    assert_eq!(sheet["mime"], "image/png");
    assert!(sheet["length"].as_u64().unwrap() > 0);

    // 4. Progressive sampling: coarse then bisect excluding coarse frames
    let coarse = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "media_sample_frames",
                &json!({"artifact_id": clip_art, "strategy": "coarse", "count": 5}),
            ),
        )
        .await,
    );
    let coarse_frames = coarse["frames"].as_array().unwrap();
    assert_eq!(coarse_frames.len(), 5);
    let coarse_indices: Vec<u64> = coarse_frames
        .iter()
        .map(|f| f["frame_index"].as_u64().unwrap())
        .collect();

    // Verify sampled frame artifact is readable via artifact_read
    let first_frame_art = coarse_frames[0]["artifact_id"].as_str().unwrap();
    let frame_chunk = payload(
        &rpc(
            port,
            &ana,
            &tool("artifact_read", &json!({"id": first_frame_art})),
        )
        .await,
    );
    assert_eq!(frame_chunk["mime"], "image/png");

    let bisect = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "media_sample_frames",
                &json!({
                    "artifact_id": clip_art,
                    "strategy": "bisect",
                    "start_ms": 0,
                    "end_ms": 700,
                    "count": 5,
                    "exclude_frames": coarse_indices
                }),
            ),
        )
        .await,
    );
    let bisect_frames = bisect["frames"].as_array().unwrap();
    assert_eq!(bisect_frames.len(), 5);
    for f in bisect_frames {
        let idx = f["frame_index"].as_u64().unwrap();
        assert!(
            !coarse_indices.contains(&idx),
            "bisect repeated frame {idx}"
        );
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn full_workspace_lifecycle_through_mcp() {
    let (port, [ana, ..]) = demo_with_artifacts().await;

    // Project lifecycle & capture settings
    let proj = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "project_create",
                &json!({"id": "int-proj", "label": "Int Initial"}),
            ),
        )
        .await,
    );
    assert_eq!(proj["id"], "int-proj");
    let renamed = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "project_update",
                &json!({"id": "int-proj", "label": "Int Renamed"}),
            ),
        )
        .await,
    );
    assert_eq!(renamed["label"], "Int Renamed");
    let settings = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_settings_update",
                &json!({"project": "int-proj", "auto_recapture": true, "base_ref": "develop"}),
            ),
        )
        .await,
    );
    assert_eq!(settings["auto_recapture"], true);

    // Publish two captures, link lineage, compare
    let png_a =
        base64::engine::general_purpose::STANDARD.encode(make_solid_png(8, 8, [10, 10, 10, 255]));
    let png_b =
        base64::engine::general_purpose::STANDARD.encode(make_solid_png(8, 8, [200, 10, 10, 255]));
    let cap1 = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({"project": "int-proj", "scenario": "s1", "members": [{"id": "home", "data_base64": png_a}]}),
            ),
        )
        .await,
    );
    let cap2 = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({"project": "int-proj", "scenario": "s1", "members": [{"id": "home", "data_base64": png_b}]}),
            ),
        )
        .await,
    );
    let (c1, c2) = (cap1["id"].as_str().unwrap(), cap2["id"].as_str().unwrap());
    let a1 = cap1["checkpoints"][0]["artifact"]["id"].as_str().unwrap();

    let parents = payload(&rpc(port, &ana, &tool("capture_parents_get", &json!({"id": c2}))).await);
    let generation = parents["generation"].as_i64().unwrap();
    let edited = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_parents_edit",
                &json!({
                    "id": c2,
                    "generation": generation,
                    "reason": "link baseline",
                    "parents": [{"id": c1, "kind": "baseline"}]
                }),
            ),
        )
        .await,
    );
    assert_eq!(edited["parents"].as_array().unwrap().len(), 1);
    let graph = payload(&rpc(port, &ana, &tool("capture_lineage", &json!({"id": c2}))).await);
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 2);
    let cmp = payload(
        &rpc(
            port,
            &ana,
            &tool("capture_compare", &json!({"before": c1, "after": c2})),
        )
        .await,
    );
    assert_eq!(cmp["changed"].as_array().unwrap().len(), 1);

    // Topic filing + selection + problem + fix + citations
    let topic = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "topic_create",
                &json!({"project": "int-proj", "path": "nav/header"}),
            ),
        )
        .await,
    );
    let tid = topic["id"].as_str().unwrap();
    let _ = payload(
        &rpc(
            port,
            &ana,
            &tool("topic_file_capture", &json!({"id": tid, "capture": c1})),
        )
        .await,
    );
    let tcaps = payload(&rpc(port, &ana, &tool("topic_captures", &json!({"id": tid}))).await);
    assert_eq!(tcaps.as_array().unwrap().len(), 1);

    let sel = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "selection_create",
                &json!({
                    "project": "int-proj",
                    "label": "Header crop",
                    "items": [{"capture": c1, "artifact": a1, "mark": "proof"}]
                }),
            ),
        )
        .await,
    );
    let sid = sel["id"].as_str().unwrap();

    let filed_problem = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "problem_create",
                &json!({
                    "project": "int-proj",
                    "title": "Header contrast",
                    "severity": "major",
                    "origin": "agent-visual",
                    "evidence": [a1]
                }),
            ),
        )
        .await,
    );
    let pid = filed_problem["id"].as_str().unwrap();
    let cited_prob = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "problem_cite_selection",
                &json!({"id": pid, "selection": sid}),
            ),
        )
        .await,
    );
    assert_eq!(cited_prob["selections"].as_array().unwrap().len(), 2);

    let detailed_probs = payload(
        &rpc(
            port,
            &ana,
            &tool("problem_list_detailed", &json!({"project": "int-proj"})),
        )
        .await,
    );
    assert_eq!(detailed_probs.as_array().unwrap().len(), 1);

    let citations = payload(&rpc(port, &ana, &tool("capture_citations", &json!({"id": c1}))).await);
    assert_ne!(citations["citations"].as_array().unwrap().len(), 0);
    assert!(citations["confirm"].is_string());
}

#[tokio::test]
async fn an_agent_files_a_problem_and_a_fix_through_mcp() {
    let (port, [ana, bruno, _]) = demo_with_artifacts().await;
    let listed = rpc(
        port,
        &ana,
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
    )
    .await;
    assert_eq!(
        listed["result"]["tools"].as_array().unwrap().len(),
        TOOLS.len()
    );

    let problem = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "problem_create",
                &json!({"project": "demo", "title": "Cramped", "severity": "minor",
                    "origin": "agent-visual", "evidence": [PROJECT_ARTIFACT]}),
            ),
        )
        .await,
    );
    let problem_id = problem["id"].as_str().unwrap();
    let fix = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "fix_create",
                &json!({"project": "demo", "title": "Space", "problems": [problem_id]}),
            ),
        )
        .await,
    );
    let approved = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "fix_review",
                &json!({"id": fix["id"], "outcome": "approved"}),
            ),
        )
        .await,
    );
    assert_eq!(approved["status"], "approved");
    let resolved = payload(
        &rpc(
            port,
            &bruno,
            &tool("problem_get", &json!({"id": problem_id})),
        )
        .await,
    );
    assert_eq!(resolved["status"], "resolved");
}

#[tokio::test]
async fn an_agent_queues_watches_and_cancels_a_capture_job_through_mcp() {
    let (port, [ana, bruno, _]) = demo_with_artifacts().await;
    payload(
        &rpc(
            port,
            &ana,
            &tool(
                "scenario_put",
                &json!({"project": "demo", "scenario": {"name": "home", "url": "/"}}),
            ),
        )
        .await,
    );
    let job = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "job_create",
                &json!({"project": "demo", "scenario": "home",
                    "trees": [{"name": "base", "base": "develop"}]}),
            ),
        )
        .await,
    );
    assert_eq!(
        (job["status"].as_str(), job["cells_total"].as_u64()),
        (Some("queued"), Some(1))
    );
    let open = payload(
        &rpc(
            port,
            &bruno,
            &tool("job_list", &json!({"project": "demo", "open": true})),
        )
        .await,
    );
    assert_eq!(open.as_array().unwrap().len(), 1);
    let cancelled = payload(&rpc(port, &ana, &tool("job_cancel", &json!({"id": job["id"]}))).await);
    assert_eq!(cancelled["status"], "cancelled");
    let fetched = payload(&rpc(port, &bruno, &tool("job_get", &json!({"id": job["id"]}))).await);
    assert_eq!(fetched["status"], "cancelled");
}

#[tokio::test]
async fn an_agent_pushes_and_reads_a_scenario_through_mcp() {
    let (port, [ana, bruno, _]) = demo_with_artifacts().await;
    let scenario = json!({"name": "home", "url": "/",
        "matrix": {"viewports": ["390x844@2"], "themes": ["dark", "light"]}});
    let put = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "scenario_put",
                &json!({"project": "demo", "scenario": scenario}),
            ),
        )
        .await,
    );
    let hash = put["hash"].as_str().unwrap().to_owned();
    let listed = payload(
        &rpc(
            port,
            &bruno,
            &tool("scenario_list", &json!({"project": "demo"})),
        )
        .await,
    );
    assert_eq!(
        (listed[0]["name"].as_str(), listed[0]["cells"].as_u64()),
        (Some("home"), Some(2))
    );
    let exact = payload(
        &rpc(
            port,
            &bruno,
            &tool(
                "scenario_get",
                &json!({"project": "demo", "name": "home", "hash": hash}),
            ),
        )
        .await,
    );
    assert_eq!(exact["scenario"]["url"], "/");
    let latest = payload(
        &rpc(
            port,
            &bruno,
            &tool("scenario_get", &json!({"project": "demo", "name": "home"})),
        )
        .await,
    );
    assert_eq!(latest["hash"], exact["hash"]);
}

#[tokio::test]
async fn an_agent_publishes_a_capture_and_files_a_problem_through_mcp() {
    let (port, [ana, _, _]) = demo_with_artifacts().await;
    let png_b64 = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nfake-png");
    let published = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({
                    "project": "demo",
                    "scenario": "mcp-shot",
                    "viewport": "1280x800",
                    "theme": "dark",
                    "members": [{"id": "home", "role": "checkpoint", "mime": "image/png", "data_base64": png_b64}]
                }),
            ),
        )
        .await,
    );
    let art_id = published["checkpoints"][0]["artifact"]["id"]
        .as_str()
        .expect("published capture returns checkpoint artifact id");
    let prob = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "problem_create",
                &json!({
                    "project": "demo",
                    "title": "Filed purely via MCP",
                    "severity": "minor",
                    "origin": "agent-visual",
                    "evidence": [art_id]
                }),
            ),
        )
        .await,
    );
    assert_eq!(prob["title"], "Filed purely via MCP");
}

#[tokio::test]
async fn tool_failures_are_results_and_bad_requests_are_rpc_errors() {
    let (port, [_, bruno, _]) = demo_with_artifacts().await;
    let denied = rpc(
        port,
        &bruno,
        &tool("problem_create", &json!({"project": "demo", "title": "x", "severity": "minor", "origin": "user-reported"})),
    )
    .await;
    assert_eq!(denied["result"]["isError"], true, "{denied}");
    let unknown = rpc(port, &bruno, &tool("nope", &json!({}))).await;
    assert!(unknown["error"]["code"].is_i64(), "{unknown}");
    let anonymous = call(port, "POST", "/mcp", None, Some("{}")).await;
    assert_eq!(status(&anonymous), StatusCode::UNAUTHORIZED, "{anonymous}");
}

/// Content uploaded in the artifact test, and the part of it read back.
const CONTENT: &str = "hello-world";
const READ_OFFSET: u64 = 6;
const READ_LENGTH: u64 = 5;
const READ_BACK: &str = "world";

#[tokio::test]
async fn artifacts_are_read_in_pages() {
    let (port, [ana, ..]) = demo_with_artifacts().await;
    let sha = stream_blob(port, &ana, "demo", CONTENT.as_bytes()).await;
    let record = capture_record("cap-mcp-read", &sha, CONTENT.len() as u64, vec![]);
    let capture = publish_record(port, &ana, &record).await;
    let capture: rupicola_api::Capture = json(&capture);
    let artifact = &capture.checkpoints[0].artifact.id;
    let read = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "artifact_read",
                &json!({"id": artifact, "offset": READ_OFFSET, "length": READ_LENGTH}),
            ),
        )
        .await,
    );
    assert_eq!(read["length"], READ_LENGTH);
    assert_eq!(read["bytes"], CONTENT.len());
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(read["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(decoded, READ_BACK.as_bytes());
}

#[tokio::test]
async fn capture_capabilities_and_selection_schemas_work() {
    const CAPTURE_SHOT: &str = "capture-shot";
    let (port, [ana, ..]) = demo_with_artifacts().await;
    let caps = payload(&rpc(port, &ana, &tool("capture_capabilities", &json!({}))).await);
    assert!(caps["backends"].is_array(), "{caps}");

    let selection = json!([{
        "label": "Proof",
        "items": [{"capture": CAPTURE_SHOT, "artifact": PROJECT_ARTIFACT, "mark": "proof"}]
    }]);
    let problem = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "problem_create",
                &json!({"project": "demo", "title": "Alignment", "severity": "minor",
                    "origin": "agent-visual", "selections": selection}),
            ),
        )
        .await,
    );
    assert_eq!(problem["selections"].as_array().unwrap().len(), 1);

    let fix = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "fix_create",
                &json!({"project": "demo", "title": "Aligned", "problems": [problem["id"]],
                    "baseline": selection, "proof": selection}),
            ),
        )
        .await,
    );
    assert_eq!(fix["selections"].as_array().unwrap().len(), 2);
}
