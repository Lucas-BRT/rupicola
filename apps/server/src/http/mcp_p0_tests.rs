//! Sprint 1 (P0) tests:
//! 1. MCP schema contract (80 tools): every `required` field exists in `properties` and a
//!    minimal payload synthesized from each tool's JSON Schema deserializes into `Call`.
//! 2. RBAC & cross-project isolation matrix (Reader mutations blocked, outsider blocked
//!    from reads/media/diffs across projects, cited capture deletion requires `confirm`).
//! 3. Concurrency, optimistic locking (`generation`), and parallel blob deduplication.

use super::tests::{make_solid_png, payload, rpc, tool};
use super::{Call, TOOLS};
use crate::testkit::*;
use base64::Engine as _;
use serde_json::{Map, Value, json};

/// Synthesizes a minimal valid JSON value matching a JSON Schema node.
fn sample_from_schema(schema: &Value) -> Value {
    if let Some(en) = schema.get("enum").and_then(Value::as_array) {
        return en.first().cloned().unwrap_or(Value::Null);
    }
    let ty = match schema.get("type") {
        Some(Value::String(s)) => s.as_str(),
        Some(Value::Array(arr)) => arr.first().and_then(Value::as_str).unwrap_or("string"),
        _ => "object",
    };
    match ty {
        "string" => Value::String("sample".to_owned()),
        "integer" => json!(1),
        "number" => json!(0.5),
        "boolean" => Value::Bool(false),
        "array" => {
            let min = schema.get("minItems").and_then(Value::as_u64).unwrap_or(0);
            if min > 0
                && let Some(item_schema) = schema.get("items")
            {
                return Value::Array(vec![sample_from_schema(item_schema)]);
            }
            Value::Array(Vec::new())
        }
        "object" => {
            let mut obj = Map::new();
            if let Some(req) = schema.get("required").and_then(Value::as_array)
                && let Some(props) = schema.get("properties").and_then(Value::as_object)
            {
                for key in req.iter().filter_map(Value::as_str) {
                    if let Some(prop_schema) = props.get(key) {
                        obj.insert(key.to_owned(), sample_from_schema(prop_schema));
                    }
                }
            }
            Value::Object(obj)
        }
        _ => Value::Null,
    }
}

#[test]
fn every_tool_schema_has_valid_required_fields_and_non_empty_enums() {
    for t in TOOLS {
        assert!(
            !t.description.trim().is_empty(),
            "{} empty description",
            t.name
        );
        let desc = t.describe();
        let props = desc["inputSchema"]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("{} missing properties object", t.name));
        for req in t.required {
            assert!(
                props.contains_key(*req),
                "tool `{}` lists required field `{req}` missing from properties",
                t.name
            );
        }
        for (prop_name, prop_schema) in props {
            if let Some(en) = prop_schema.get("enum").and_then(Value::as_array) {
                assert_ne!(
                    en.len(),
                    0,
                    "tool `{}` property `{prop_name}` has empty enum",
                    t.name
                );
            }
        }
    }
}

#[test]
fn minimal_valid_payload_from_schema_deserializes_into_call() {
    for t in TOOLS {
        let desc = t.describe();
        let mut args = sample_from_schema(&desc["inputSchema"]);
        // Special-case tagged union `media_sample_frames` where `strategy: "coarse"` needs `count`.
        if t.name == "media_sample_frames" {
            args["count"] = json!(5);
        }
        let envelope = json!({"name": t.name, "arguments": args});
        let parsed = serde_json::from_value::<Call>(envelope.clone());
        assert!(
            parsed.is_ok(),
            "tool `{}` failed to deserialize minimal schema payload `{envelope}`: {:?}",
            t.name,
            parsed.err()
        );
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn rbac_matrix_blocks_reader_mutations_and_cross_project_access() {
    let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
    let png_demo =
        base64::engine::general_purpose::STANDARD.encode(make_solid_png(8, 8, [30, 30, 30, 255]));
    let png_other =
        base64::engine::general_purpose::STANDARD.encode(make_solid_png(8, 8, [200, 30, 30, 255]));

    // Seed one capture in `demo` (ana) and one in `other` (carla)
    let cap_demo = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({"project": "demo", "scenario": "s-demo", "members": [{"id": "home", "data_base64": png_demo}]}),
            ),
        )
        .await,
    );
    let cap_other = payload(
        &rpc(
            port,
            &carla,
            &tool(
                "capture_publish",
                &json!({"project": "other", "scenario": "s-other", "members": [{"id": "home", "data_base64": png_other}]}),
            ),
        )
        .await,
    );
    let demo_cap_id = cap_demo["id"].as_str().unwrap();
    let demo_art_id = cap_demo["checkpoints"][0]["artifact"]["id"]
        .as_str()
        .unwrap();
    let other_art_id = cap_other["checkpoints"][0]["artifact"]["id"]
        .as_str()
        .unwrap();

    // 1. Reader (bruno) must be denied every mutation on `demo`
    let reader_mutations = [
        tool(
            "capture_publish",
            &json!({"project": "demo", "scenario": "nope", "members": [{"id": "x", "data_base64": png_demo}]}),
        ),
        tool(
            "capture_review",
            &json!({"id": demo_cap_id, "note": "unauthorized review"}),
        ),
        tool(
            "problem_create",
            &json!({"project": "demo", "title": "Nope", "severity": "minor", "origin": "agent-visual", "evidence": [demo_art_id]}),
        ),
        tool(
            "topic_create",
            &json!({"project": "demo", "path": "forbidden"}),
        ),
        tool("design_system_create", &json!({"project": "demo"})),
        tool(
            "capture_settings_update",
            &json!({"project": "demo", "auto_recapture": true}),
        ),
        tool(
            "project_update",
            &json!({"id": "demo", "label": "Hijacked"}),
        ),
        tool("project_delete", &json!({"id": "demo"})),
    ];
    for req in reader_mutations {
        let reply = rpc(port, &bruno, &req).await;
        assert_eq!(
            reply["result"]["isError"], true,
            "reader bruno should be denied {req}: {reply}"
        );
    }

    // 2. Outsider (carla, owner of `other`) must be denied all reads & media ops on `demo`
    let cross_project_calls = [
        tool("capture_get", &json!({"id": demo_cap_id})),
        tool("artifact_read", &json!({"id": demo_art_id})),
        tool("media_probe", &json!({"artifact_id": demo_art_id})),
        tool(
            "visual_crop_region",
            &json!({"artifact_id": demo_art_id, "x": 0, "y": 0, "width": 4, "height": 4}),
        ),
        tool(
            "visual_diff_images",
            &json!({"before_artifact_id": demo_art_id, "after_artifact_id": other_art_id}),
        ),
        tool("problem_list", &json!({"project": "demo"})),
        tool("topic_tree", &json!({"project": "demo"})),
    ];
    for req in cross_project_calls {
        let reply = rpc(port, &carla, &req).await;
        assert_eq!(
            reply["result"]["isError"], true,
            "outsider carla should be denied {req}: {reply}"
        );
    }

    // 3. Cited capture deletion requires confirmation token
    let _prob = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "problem_create",
                &json!({"project": "demo", "title": "Cites cap", "severity": "minor", "origin": "agent-visual", "evidence": [demo_art_id]}),
            ),
        )
        .await,
    );
    let unconfirmed = rpc(
        port,
        &ana,
        &tool(
            "capture_delete",
            &json!({"id": demo_cap_id, "reason": "cleanup without confirm"}),
        ),
    )
    .await;
    assert_eq!(
        unconfirmed["result"]["isError"], true,
        "deleting cited capture without confirm must fail: {unconfirmed}"
    );
    let citations = payload(
        &rpc(
            port,
            &ana,
            &tool("capture_citations", &json!({"id": demo_cap_id})),
        )
        .await,
    );
    let confirm = citations["confirm"].as_str().unwrap();
    let confirmed = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_delete",
                &json!({"id": demo_cap_id, "reason": "confirmed cleanup", "confirm": confirm}),
            ),
        )
        .await,
    );
    assert_eq!(confirmed["deleted"], demo_cap_id);
}

#[tokio::test]
async fn concurrent_job_claim_allows_only_one_winner() {
    let (port, [ana, ..]) = demo_with_artifacts().await;
    let scenario = json!({"name": "race", "url": "/"});
    let _ = payload(
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
    let created = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "job_create",
                &json!({"project": "demo", "scenario": "race", "trees": [{"name": "base", "base": "develop"}]}),
            ),
        )
        .await,
    );
    let job_id = created["id"].as_str().unwrap().to_owned();

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let token = ana.clone();
        set.spawn(async move {
            payload(
                &rpc(
                    port,
                    &token,
                    &tool("job_claim", &json!({"project": "demo"})),
                )
                .await,
            )
        });
    }
    let mut winners = Vec::new();
    let mut empty = 0usize;
    while let Some(res) = set.join_next().await {
        let val = res.unwrap();
        if val.is_null() {
            empty += 1;
        } else {
            winners.push(val["id"].as_str().unwrap().to_owned());
        }
    }
    assert_eq!(winners, vec![job_id]);
    assert_eq!(empty, 7);
}

#[tokio::test]
async fn optimistic_locking_rejects_stale_generation_edits() {
    let (port, [ana, ..]) = demo_with_artifacts().await;

    // 1. Topic rename optimistic locking
    let topic = payload(
        &rpc(
            port,
            &ana,
            &tool("topic_create", &json!({"project": "demo", "path": "cards"})),
        )
        .await,
    );
    let tid = topic["id"].as_str().unwrap();
    let stale_gen = topic["generation"].as_i64().unwrap();
    let first = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "topic_rename",
                &json!({"id": tid, "name": "cards-v2", "generation": stale_gen}),
            ),
        )
        .await,
    );
    assert_eq!(first["name"], "cards-v2");
    let conflict = rpc(
        port,
        &ana,
        &tool(
            "topic_rename",
            &json!({"id": tid, "name": "cards-v3", "generation": stale_gen}),
        ),
    )
    .await;
    assert_eq!(
        conflict["result"]["isError"], true,
        "stale topic generation must conflict: {conflict}"
    );

    // 2. Capture parents optimistic locking
    let png =
        base64::engine::general_purpose::STANDARD.encode(make_solid_png(4, 4, [10, 20, 30, 255]));
    let c1 = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({"project": "demo", "scenario": "s1", "members": [{"id": "a", "data_base64": png}]}),
            ),
        )
        .await,
    );
    let c2 = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_publish",
                &json!({"project": "demo", "scenario": "s1", "members": [{"id": "b", "data_base64": png}]}),
            ),
        )
        .await,
    );
    let (id1, id2) = (c1["id"].as_str().unwrap(), c2["id"].as_str().unwrap());
    let parents = payload(
        &rpc(
            port,
            &ana,
            &tool("capture_parents_get", &json!({"id": id2})),
        )
        .await,
    );
    let stale_parent_gen = parents["generation"].as_i64().unwrap();
    let _ = payload(
        &rpc(
            port,
            &ana,
            &tool(
                "capture_parents_edit",
                &json!({"id": id2, "generation": stale_parent_gen, "reason": "first", "parents": [{"id": id1, "kind": "baseline"}]}),
            ),
        )
        .await,
    );
    let stale_edit = rpc(
        port,
        &ana,
        &tool(
            "capture_parents_edit",
            &json!({"id": id2, "generation": stale_parent_gen, "reason": "stale", "parents": []}),
        ),
    )
    .await;
    assert_eq!(
        stale_edit["result"]["isError"], true,
        "stale capture_parents generation must conflict: {stale_edit}"
    );
}

#[tokio::test]
async fn concurrent_identical_blob_publishes_deduplicate_safely() {
    let (port, [ana, ..]) = demo_with_artifacts().await;
    let raw_png = make_solid_png(10, 10, [99, 88, 77, 255]);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&raw_png);

    let mut set = tokio::task::JoinSet::new();
    for i in 0..8 {
        let token = ana.clone();
        let data = b64.clone();
        set.spawn(async move {
            payload(
                &rpc(
                    port,
                    &token,
                    &tool(
                        "capture_publish",
                        &json!({
                            "project": "demo",
                            "scenario": format!("parallel-{i}"),
                            "members": [{"id": "shot", "data_base64": data}]
                        }),
                    ),
                )
                .await,
            )
        });
    }

    let mut artifact_ids = Vec::new();
    while let Some(res) = set.join_next().await {
        let cap = res.unwrap();
        artifact_ids.push(
            cap["checkpoints"][0]["artifact"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    assert_eq!(artifact_ids.len(), 8);

    // Every published artifact must read back the exact original PNG bytes.
    let chunk = payload(
        &rpc(
            port,
            &ana,
            &tool("artifact_read", &json!({"id": artifact_ids[0]})),
        )
        .await,
    );
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(chunk["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(decoded, raw_png);
}
