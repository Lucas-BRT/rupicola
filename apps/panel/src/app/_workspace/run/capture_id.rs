//! `GET` and `POST /run/{capture_id}`: one capture and its review.

use crate::app::{_workspace::title, account, fail, panel, respond};
use crate::{
    Account, Deserialize, PageResult, Panel, Run, StatusCode, auth,
    auth::Role,
    media, redirect, runs,
    views::run::{Capture, run_page},
};
use rupicola_api::{
    CaptureRemoval, CaptureStatus, MotionDiffReport, NewFinding, NewSelection, SelectionItem,
    SelectionMark,
};
use std::collections::BTreeMap;
use topcoat::{
    Result,
    context::Cx,
    metadata::Metadata,
    router::{content::Form, module_param, page, parse_query_params, response::Response, route},
    view::{View, view},
};

module_param!(pub capture_id);

#[derive(Deserialize)]
struct Comparison {
    before: Option<String>,
    motion: Option<String>,
    diff_art: Option<String>,
}

#[page(with(Metadata::from_fn(async |cx| {
    let run = runs::find(cx, capture_id(cx)).await.map_err(fail)?;
    Ok(Metadata::new().title(title(&run.scenario)))
})))]
async fn page(cx: &Cx) -> Result<impl View> {
    let comparison: Comparison = parse_query_params(cx)?;
    let data = load(cx, comparison, capture_id(cx).to_owned())
        .await
        .map_err(fail)?;
    Ok(view! { run_page(data: &data) })
}

async fn load(cx: &Cx, comparison: Comparison, id: String) -> PageResult<Capture> {
    let found = runs::find(cx, &id).await?;
    let key = found.key;
    let project_label = found.project;
    let run = found.capture;
    let role = auth::role_of(cx, &run.project_id).await?;
    let candidates: Vec<_> = runs::summaries(cx, &run.project_id)
        .await?
        .iter()
        .filter(|entry| {
            entry.scenario == run.scenario
                && entry.status == CaptureStatus::Complete
                && entry.id != key
        })
        .cloned()
        .collect();
    let requested = comparison.before;
    let before = match requested
        .as_deref()
        .filter(|before| candidates.iter().any(|candidate| candidate.id == *before))
    {
        Some(before) => {
            let before = runs::find(cx, before).await?.capture;
            let differs = before.environment != run.environment;
            Some((before, differs))
        }
        None => None,
    };
    let environments_differ = before.as_ref().is_some_and(|(_, differs)| *differs);
    let (before_images, before_artifact_ids) =
        before_checkpoint_maps(before.as_ref().map(|(c, _)| c));
    let (p, acc) = (panel(cx), account(cx));
    let citations = if role == Role::Reader {
        None
    } else {
        p.api.capture_citations(&acc.session, &key).await.ok()
    };
    let lineage = p
        .api
        .capture_lineage(&acc.session, &key, Some(2))
        .await
        .ok();
    let diff = if let Some((before_cap, _)) = &before {
        p.api
            .compare_captures(&acc.session, &before_cap.id, &key)
            .await
            .ok()
    } else {
        None
    };
    let topics = p
        .api
        .topic_tree(&acc.session, &run.project_id)
        .await
        .map(|t| t.topics)
        .unwrap_or_default();
    let open_problems = if role == Role::Reader {
        Vec::new()
    } else {
        p.api
            .problems(&acc.session, &run.project_id)
            .await
            .unwrap_or_default()
    };
    let motion_report = if let Some(art_id) = comparison.motion.as_deref() {
        fetch_motion_diff(&p, &acc, art_id).await
    } else {
        None
    };
    Ok(Capture {
        width: run.environment.width.unwrap_or(640).max(1).to_string(),
        height: run.environment.height.unwrap_or(480).max(1).to_string(),
        may_review: role != Role::Reader,
        comparing: before.is_some(),
        project_url: format!("/projects/{}", run.project_id),
        project_label,
        form_url: format!("/run/{key}"),
        record: run,
        candidates,
        requested,
        before_images,
        before_artifact_ids,
        environments_differ,
        citations,
        lineage,
        diff,
        diff_artifact_id: comparison.diff_art,
        motion_report,
        topics,
        open_problems,
    })
}

fn before_checkpoint_maps(
    before: Option<&rupicola_api::Capture>,
) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut images = BTreeMap::new();
    let mut artifact_ids = BTreeMap::new();
    if let Some(cap) = before {
        for item in &cap.checkpoints {
            images.insert(item.id.clone(), media::url(&item.artifact.id));
            artifact_ids.insert(item.id.clone(), item.artifact.id.clone());
        }
    }
    (images, artifact_ids)
}

async fn fetch_motion_diff(
    panel: &Panel,
    account: &Account,
    artifact_id: &str,
) -> Option<MotionDiffReport> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "media_motion_diff",
            "arguments": {"artifact_id": artifact_id, "threshold": 0.12}
        }
    })
    .to_string();
    let reply = panel.api.mcp(&account.session, &payload).await.ok()?;
    let body: serde_json::Value = serde_json::from_str(&reply.body).ok()?;
    if body["result"]["isError"].as_bool() == Some(true) {
        return None;
    }
    let text = body["result"]["content"][0]["text"].as_str()?;
    serde_json::from_str(text).ok()
}

#[route(POST)]
async fn inspect(cx: &Cx, Form(form): Form<Review>) -> Result<Response> {
    respond(cx, review(cx, capture_id(cx).into(), form).await)
}

#[derive(Deserialize)]
struct Review {
    csrf: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    note: String,
    evidence: Option<String>,
    #[serde(default)]
    reason: String,
    confirm: Option<String>,
    #[serde(default)]
    topic_id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    artifact_id: String,
    #[serde(default)]
    problem_id: String,
    #[serde(default)]
    before_artifact_id: String,
    #[serde(default)]
    after_artifact_id: String,
    #[serde(default)]
    before_capture_id: String,
    #[serde(default)]
    mode: String,
}

async fn review(cx: &Cx, id: String, form: Review) -> PageResult<Response> {
    let (panel, account) = (panel(cx), account(cx));
    auth::check_csrf(&account, &form.csrf)?;
    let run = runs::find(cx, &id).await?;
    match form.action.as_str() {
        "delete" => {
            let reason = form.reason.trim();
            if reason.is_empty() {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            panel
                .api
                .delete_capture(
                    &account.session,
                    &id,
                    &CaptureRemoval {
                        reason: reason.to_owned(),
                        confirm: form.confirm.filter(|c| !c.trim().is_empty()),
                    },
                )
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(redirect(&format!(
                "/projects/{}/captures",
                run.capture.project_id
            )))
        }
        "file-topic" => {
            panel
                .api
                .file_topic_capture(&account.session, form.topic_id.trim(), &id)
                .await
                .map_err(|e| auth::api_error(&e))?;
            Ok(redirect(&format!("/run/{id}#organize")))
        }
        "create-selection" => {
            create_selection_action(&panel, &account, &run, &id, &form).await?;
            Ok(redirect(&format!("/run/{id}#organize")))
        }
        "visual-diff" => {
            let diff_art = render_visual_diff(&panel, &account, &form).await?;
            Ok(redirect(&format!(
                "/run/{id}?before={}&diff_art={diff_art}#visual-diff",
                form.before_capture_id.trim(),
            )))
        }
        _ => {
            record_inspection(&panel, &account, &run, &form.note, form.evidence.as_deref()).await?;
            Ok(redirect(&format!("/run/{id}#review")))
        }
    }
}

async fn create_selection_action(
    panel: &Panel,
    account: &Account,
    run: &Run,
    capture_id: &str,
    form: &Review,
) -> PageResult<()> {
    let label = form.label.trim();
    if label.is_empty() || form.artifact_id.trim().is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let sel = panel
        .api
        .create_selection(
            &account.session,
            &run.capture.project_id,
            &NewSelection {
                label: label.to_owned(),
                items: vec![SelectionItem {
                    capture: capture_id.to_owned(),
                    artifact: Some(form.artifact_id.trim().to_owned()),
                    path: None,
                    mark: SelectionMark::Proof,
                    fragment: None,
                }],
            },
        )
        .await
        .map_err(|e| auth::api_error(&e))?;
    let prob_id = form.problem_id.trim();
    if !prob_id.is_empty() {
        panel
            .api
            .cite_problem_selection(&account.session, prob_id, &sel.id)
            .await
            .map_err(|e| auth::api_error(&e))?;
    }
    Ok(())
}

async fn render_visual_diff(panel: &Panel, account: &Account, form: &Review) -> PageResult<String> {
    let mode = match form.mode.as_str() {
        "heatmap" => "heatmap",
        "side_by_side" => "side_by_side",
        _ => "highlight",
    };
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "visual_diff_images",
            "arguments": {
                "before_artifact_id": form.before_artifact_id.trim(),
                "after_artifact_id": form.after_artifact_id.trim(),
                "mode": mode
            }
        }
    })
    .to_string();
    let reply = panel
        .api
        .mcp(&account.session, &payload)
        .await
        .map_err(|e| auth::api_error(&e))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&reply.body).map_err(|_| StatusCode::BAD_GATEWAY)?;
    let text = parsed["result"]["content"][0]["text"]
        .as_str()
        .ok_or(StatusCode::BAD_GATEWAY)?;
    let report: rupicola_api::VisualDiffReport =
        serde_json::from_str(text).map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(report.diff_artifact_id)
}

/// Records a review note on a capture; the evidence is the id of one of its artifacts.
async fn record_inspection(
    panel: &Panel,
    account: &Account,
    run: &Run,
    note: &str,
    evidence: Option<&str>,
) -> PageResult<()> {
    let note = note.trim();
    let evidence = evidence.filter(|artifact| !artifact.is_empty());
    if note.is_empty()
        || note.len() > 8000
        || evidence
            .as_ref()
            .is_some_and(|artifact| !media::declared(&run.capture, artifact))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    panel
        .api
        .add_finding(
            &account.session,
            &run.key,
            &NewFinding {
                note: note.to_owned(),
                evidence_artifact_id: evidence.map(str::to_owned),
            },
        )
        .await
        .map(|_| ())
        .map_err(|e| auth::api_error(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::{
        Artifact, Capture as ApiCapture, Checkpoint, Environment, Inspection, MotionSpike, Problem,
        ProblemOrigin, ProblemStatus, Selection, Severity, VisualDiffReport,
    };
    use rupicola_client::{McpReply, MockApi};

    fn test_account() -> Account {
        Account {
            handle: "rev".to_owned(),
            owner: false,
            session: "sess-1".to_owned(),
            csrf: "csrf-ok".to_owned(),
        }
    }

    fn sample_run() -> Run {
        let capture = ApiCapture {
            id: "cap-1".to_owned(),
            project_id: "demo".to_owned(),
            scenario: "home".to_owned(),
            revision: None,
            status: CaptureStatus::Complete,
            blocker: None,
            inspection: Inspection::Pending,
            environment: Environment::default(),
            created_at: 0,
            checkpoints: vec![Checkpoint {
                id: "hero".to_owned(),
                artifact: Artifact {
                    id: "art-1".to_owned(),
                    path: "checkpoints/hero.png".to_owned(),
                    mime: "image/png".to_owned(),
                    bytes: 100,
                    sha256: "0".repeat(64),
                },
            }],
            clips: Vec::new(),
            findings: Vec::new(),
            deleted: None,
            plan_spec: None,
            plan_digest: None,
        };
        Run {
            key: "cap-1".to_owned(),
            project_id: "demo".to_owned(),
            project: "Demo".to_owned(),
            scenario: "home".to_owned(),
            status: "complete".to_owned(),
            inspection: "pending".to_owned(),
            created_at_ms: 0,
            capture,
        }
    }

    fn empty_review() -> Review {
        Review {
            csrf: "csrf-ok".to_owned(),
            action: String::new(),
            note: String::new(),
            evidence: None,
            reason: String::new(),
            confirm: None,
            topic_id: String::new(),
            label: String::new(),
            artifact_id: String::new(),
            problem_id: String::new(),
            before_artifact_id: String::new(),
            after_artifact_id: String::new(),
            before_capture_id: String::new(),
            mode: String::new(),
        }
    }

    #[test]
    fn before_checkpoint_maps_extracts_urls_and_artifact_ids() {
        let run = sample_run();
        let (images, ids) = before_checkpoint_maps(Some(&run.capture));
        assert_eq!(
            images.get("hero").map(String::as_str),
            Some("/media/a/art-1")
        );
        assert_eq!(ids.get("hero").map(String::as_str), Some("art-1"));

        let (empty_imgs, empty_ids) = before_checkpoint_maps(None);
        assert!(empty_imgs.is_empty() && empty_ids.is_empty());
    }

    #[tokio::test]
    async fn create_selection_action_handles_standalone_attached_and_invalid() {
        let mut api = MockApi::new();
        api.expect_create_selection()
            .times(2)
            .returning(|_, _, form| {
                Ok(Selection {
                    id: "sel-1".to_owned(),
                    label: form.label.clone(),
                    items: form.items.clone(),
                    author: None,
                    created_at: 0,
                })
            });
        api.expect_cite_problem_selection()
            .times(1)
            .withf(|_, prob, sel| prob == "prob-5" && sel == "sel-1")
            .returning(|_, _, _| {
                Ok(Problem {
                    id: "prob-5".to_owned(),
                    project_id: "demo".to_owned(),
                    title: "T".to_owned(),
                    status: ProblemStatus::Open,
                    severity: Severity::Minor,
                    origin: ProblemOrigin::UserReported,
                    route: String::new(),
                    viewport: String::new(),
                    theme: String::new(),
                    diagnosis: String::new(),
                    proposed_solution: String::new(),
                    rules: Vec::new(),
                    evidence: Vec::new(),
                    selections: Vec::new(),
                    comments: Vec::new(),
                    author: None,
                    created_at: 0,
                    updated_at: 0,
                })
            });

        let panel = Panel::for_test(api);
        let acc = test_account();
        let run = sample_run();

        assert_eq!(
            create_selection_action(&panel, &acc, &run, "cap-1", &empty_review()).await,
            Err(StatusCode::UNPROCESSABLE_ENTITY)
        );

        let mut standalone = empty_review();
        standalone.label = "Header proof".to_owned();
        standalone.artifact_id = "art-1".to_owned();
        create_selection_action(&panel, &acc, &run, "cap-1", &standalone)
            .await
            .unwrap();

        let mut attached = standalone;
        attached.problem_id = "prob-5".to_owned();
        create_selection_action(&panel, &acc, &run, "cap-1", &attached)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn render_visual_diff_and_fetch_motion_diff_parse_mcp_replies() {
        let mut api = MockApi::new();
        let diff_json = serde_json::to_string(&VisualDiffReport {
            before_artifact_id: "art-0".to_owned(),
            after_artifact_id: "art-1".to_owned(),
            diff_artifact_id: "art-diff-9".to_owned(),
            changed_ratio: 0.0042,
            changed_regions: Vec::new(),
        })
        .unwrap();
        let motion_json = serde_json::to_string(&MotionDiffReport {
            artifact_id: "art-vid-1".to_owned(),
            threshold: 0.1,
            spikes: vec![MotionSpike {
                frame_index: 4,
                timestamp_ms: 1250,
                delta_ratio: 0.25,
                luma_delta: 0.42,
            }],
        })
        .unwrap();

        api.expect_mcp().times(1).returning(move |_, payload| {
            assert!(payload.contains("\"mode\":\"highlight\""));
            Ok(McpReply {
                status: 200,
                body: serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": { "content": [{ "type": "text", "text": diff_json }] }
                })
                .to_string(),
            })
        });
        api.expect_mcp().times(1).returning(move |_, _| {
            Ok(McpReply {
                status: 200,
                body: serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": { "content": [{ "type": "text", "text": motion_json }] }
                })
                .to_string(),
            })
        });
        api.expect_mcp().times(1).returning(|_, _| {
            Ok(McpReply {
                status: 200,
                body: "not-json".to_owned(),
            })
        });

        let panel = Panel::for_test(api);
        let acc = test_account();

        let mut form = empty_review();
        form.before_artifact_id = "art-0".to_owned();
        form.after_artifact_id = "art-1".to_owned();
        form.mode = "unknown-mode".to_owned();
        assert_eq!(
            render_visual_diff(&panel, &acc, &form).await.unwrap(),
            "art-diff-9"
        );

        let report = fetch_motion_diff(&panel, &acc, "art-vid-1").await.unwrap();
        assert_eq!(report.spikes.len(), 1);
        assert_eq!(report.spikes[0].timestamp_ms, 1250);

        assert_eq!(
            render_visual_diff(&panel, &acc, &form).await,
            Err(StatusCode::BAD_GATEWAY)
        );
    }
}
