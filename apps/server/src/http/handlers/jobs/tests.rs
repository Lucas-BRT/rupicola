use crate::testkit::*;
use axum::http::StatusCode;
use rupicola_api::{IssuedCredential, Job, JobStatus};

const HOME: &str = r#"{"name":"home","url":"/",
    "matrix":{"viewports":["390x844","1280x800"],"themes":["dark","light"]}}"#;
const TREES: &str = r#"[{"name":"base","base":"develop"},
    {"name":"fixed","base":"develop","merge":["fix/spacing"]}]"#;
/// The capture that holds the demo project's artifact.
const CAPTURE: &str = "capture-shot";

async fn scenario_and_job(port: u16, token: &str) -> Job {
    let pushed = call(
        port,
        "POST",
        "/v1/projects/demo/scenarios",
        Some(token),
        Some(HOME),
    )
    .await;
    assert!(status(&pushed).is_success(), "{pushed}");
    let created = call(
        port,
        "POST",
        "/v1/projects/demo/jobs",
        Some(token),
        Some(&format!(r#"{{"scenario":"home","trees":{TREES}}}"#)),
    )
    .await;
    assert_eq!(status(&created), StatusCode::CREATED, "{created}");
    json(&created)
}

async fn device(port: u16, owner: &str) -> String {
    let issued = call(
        port,
        "POST",
        "/v1/projects/demo/devices",
        Some(owner),
        Some(r#"{"label":"runner"}"#),
    )
    .await;
    assert_eq!(status(&issued), StatusCode::CREATED, "{issued}");
    json::<IssuedCredential>(&issued).token
}

async fn claim(port: u16, token: &str) -> String {
    call(
        port,
        "POST",
        "/v1/projects/demo/jobs/claim",
        Some(token),
        None,
    )
    .await
}

#[tokio::test]
async fn a_runner_takes_a_job_reports_progress_and_finishes_it() {
    let (port, [ana, bruno, _]) = demo_with_artifacts().await;
    let runner = device(port, &ana).await;
    let job = scenario_and_job(port, &ana).await;
    assert_eq!(job.status, JobStatus::Queued);
    assert_eq!(job.cells_total, 8, "four captures on each of two trees");
    assert_eq!(job.trees.len(), 2);

    let taken = claim(port, &runner).await;
    assert_eq!(status(&taken), StatusCode::OK, "{taken}");
    let held: Job = json(&taken);
    assert_eq!(
        (held.id.as_str(), held.status),
        (job.id.as_str(), JobStatus::Running)
    );
    assert_eq!(status(&claim(port, &runner).await), StatusCode::NO_CONTENT);

    let path = format!("/v1/jobs/{}", job.id);
    let progress = call(
        port,
        "POST",
        &format!("{path}/progress"),
        Some(&runner),
        Some(r#"{"cells_done":3}"#),
    )
    .await;
    assert_eq!(json::<Job>(&progress).cells_done, 3, "{progress}");
    let too_far = call(
        port,
        "POST",
        &format!("{path}/progress"),
        Some(&runner),
        Some(r#"{"cells_done":9}"#),
    )
    .await;
    assert_eq!(
        status(&too_far),
        StatusCode::UNPROCESSABLE_ENTITY,
        "{too_far}"
    );

    let watching = call(
        port,
        "GET",
        "/v1/projects/demo/jobs?open=true",
        Some(&bruno),
        None,
    )
    .await;
    let open: Vec<Job> = json(&watching);
    assert_eq!(open.len(), 1);
    assert_eq!((open[0].cells_done, open[0].cells_total), (3, 8));

    let done = call(
        port,
        "POST",
        &format!("{path}/complete"),
        Some(&runner),
        Some(&format!(
            r#"{{"captures":[{{"capture":"{CAPTURE}","tree":"base","viewport":"390x844","theme":"dark"}}]}}"#
        )),
    )
    .await;
    let done: Job = json(&done);
    assert_eq!((done.status, done.cells_done), (JobStatus::Done, 8));
    assert_eq!(done.captures.len(), 1);
    assert_eq!(done.captures[0].tree, "base");
    let still_open: Vec<Job> = json(
        &call(
            port,
            "GET",
            "/v1/projects/demo/jobs?open=true",
            Some(&ana),
            None,
        )
        .await,
    );
    assert_eq!(still_open.len(), 0);
}

#[tokio::test]
async fn only_the_runner_that_holds_a_job_reports_on_it_and_cancelling_stops_it() {
    let (port, [ana, bruno, _]) = demo_with_artifacts().await;
    let holder = device(port, &ana).await;
    let other = device(port, &ana).await;
    let job = scenario_and_job(port, &ana).await;
    claim(port, &holder).await;
    let path = format!("/v1/jobs/{}", job.id);

    let stranger = call(
        port,
        "POST",
        &format!("{path}/progress"),
        Some(&other),
        Some(r#"{"cells_done":1}"#),
    )
    .await;
    assert_eq!(status(&stranger), StatusCode::CONFLICT, "{stranger}");
    let empty_error = call(
        port,
        "POST",
        &format!("{path}/fail"),
        Some(&holder),
        Some(r#"{"error":" "}"#),
    )
    .await;
    assert_eq!(
        status(&empty_error),
        StatusCode::UNPROCESSABLE_ENTITY,
        "{empty_error}"
    );

    let foreign = call(
        port,
        "POST",
        &format!("{path}/complete"),
        Some(&holder),
        Some(r#"{"captures":[{"capture":"capture-foreign-shot","tree":"base","viewport":"390x844"}]}"#),
    )
    .await;
    assert_eq!(
        status(&foreign),
        StatusCode::UNPROCESSABLE_ENTITY,
        "{foreign}"
    );

    let denied = call(port, "POST", &format!("{path}/cancel"), Some(&bruno), None).await;
    assert!(status(&denied).is_client_error(), "{denied}");
    let cancelled = call(port, "POST", &format!("{path}/cancel"), Some(&ana), None).await;
    assert_eq!(
        json::<Job>(&cancelled).status,
        JobStatus::Cancelled,
        "{cancelled}"
    );

    let heartbeat = call(
        port,
        "POST",
        &format!("{path}/progress"),
        Some(&holder),
        Some(r#"{"cells_done":2}"#),
    )
    .await;
    assert_eq!(
        json::<Job>(&heartbeat).status,
        JobStatus::Cancelled,
        "the holder learns to stop"
    );
    let again = call(port, "POST", &format!("{path}/cancel"), Some(&ana), None).await;
    assert_eq!(status(&again), StatusCode::CONFLICT, "{again}");
}

#[tokio::test]
async fn a_job_needs_an_existing_scenario_and_a_person_to_ask_for_it() {
    let (port, [ana, bruno, _]) = demo_with_artifacts().await;
    let runner = device(port, &ana).await;
    let body = format!(r#"{{"scenario":"home","trees":{TREES}}}"#);
    let missing = call(
        port,
        "POST",
        "/v1/projects/demo/jobs",
        Some(&ana),
        Some(&body),
    )
    .await;
    assert_eq!(
        status(&missing),
        StatusCode::UNPROCESSABLE_ENTITY,
        "{missing}"
    );
    scenario_and_job(port, &ana).await;
    for token in [&runner, &bruno] {
        let denied = call(
            port,
            "POST",
            "/v1/projects/demo/jobs",
            Some(token),
            Some(&body),
        )
        .await;
        assert!(status(&denied).is_client_error(), "{denied}");
    }
    let unknown_hash = format!(r#"{{"scenario":"home","hash":"nope","trees":{TREES}}}"#);
    let refused = call(
        port,
        "POST",
        "/v1/projects/demo/jobs",
        Some(&ana),
        Some(&unknown_hash),
    )
    .await;
    assert_eq!(
        status(&refused),
        StatusCode::UNPROCESSABLE_ENTITY,
        "{refused}"
    );
}

fn make_solid_png(rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let data: Vec<u8> = rgba.into_iter().cycle().take(16).collect();
    writer.write_image_data(&data).unwrap();
    drop(writer);
    out
}

async fn upload_png_capture(port: u16, token: &str, revision: &str, rgba: [u8; 4]) -> String {
    let bytes = make_solid_png(rgba);
    let sha = stream_blob(port, token, "demo", &bytes).await;
    let id = uuid::Uuid::now_v7().to_string();
    let mut record = capture_record(&id, &sha, bytes.len() as u64, vec![]);
    record.scenario.name = "home".to_owned();
    record.revision = Some(revision.to_owned());
    record.members[0].path = "checkpoints/full.png".to_owned();
    record.members[0].owner = "full".to_owned();
    let created = publish_record(port, token, &record).await;
    assert_eq!(status(&created), StatusCode::CREATED, "{created}");
    json::<rupicola_api::Capture>(&created).id
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn fix_job_pairs_isolated_and_integrated_diffs_and_approval_triggers_recapture() {
    let (port, [ana, _, _]) = demo_with_artifacts().await;
    let runner = device(port, &ana).await;

    let scen = call(
        port,
        "POST",
        "/v1/projects/demo/scenarios",
        Some(&ana),
        Some(HOME),
    )
    .await;
    assert_eq!(status(&scen), StatusCode::CREATED, "{scen}");

    let prob_body = format!(
        r#"{{"title":"Spacing off","severity":"major","origin":"agent-visual","evidence":["{PROJECT_ARTIFACT}"]}}"#
    );
    let prob = call(
        port,
        "POST",
        "/v1/projects/demo/problems",
        Some(&ana),
        Some(&prob_body),
    )
    .await;
    let prob_id = json::<rupicola_api::Problem>(&prob).id;

    let fix_a: rupicola_api::Fix = json(
        &call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&format!(
                r#"{{"title":"Fix A","git_ref":"fix/a","problems":["{prob_id}"]}}"#
            )),
        )
        .await,
    );
    let fix_b: rupicola_api::Fix = json(
        &call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&format!(
                r#"{{"title":"Fix B","git_ref":"fix/b","problems":["{prob_id}"]}}"#
            )),
        )
        .await,
    );

    let initial_trees = serde_json::json!([
        {"name": "base", "base": "develop", "merge": [], "fixes": []},
        {"name": "fix", "base": "develop", "merge": ["fix/b"], "fixes": []},
        {"name": "approved", "base": "develop", "merge": [], "fixes": []},
        {"name": "approved-fix", "base": "develop", "merge": ["fix/b"], "fixes": []}
    ]);
    let job_b: Job = json(
        &call(
            port,
            "POST",
            "/v1/projects/demo/jobs",
            Some(&ana),
            Some(
                &serde_json::json!({
                    "scenario": "home",
                    "fix": fix_b.id,
                    "trees": initial_trees
                })
                .to_string(),
            ),
        )
        .await,
    );
    let claimed = call(
        port,
        "POST",
        "/v1/projects/demo/jobs/claim",
        Some(&runner),
        None,
    )
    .await;
    assert_eq!(status(&claimed), StatusCode::OK);

    let c_base = upload_png_capture(port, &runner, "rev-base", [10, 10, 10, 255]).await;
    let c_fix = upload_png_capture(port, &runner, "rev-fix", [200, 10, 10, 255]).await;
    let c_app = upload_png_capture(port, &runner, "rev-base", [10, 10, 10, 255]).await;
    let c_app_fix = upload_png_capture(port, &runner, "rev-app-fix", [200, 10, 10, 255]).await;

    let done = call(
        port,
        "POST",
        &format!("/v1/jobs/{}/complete", job_b.id),
        Some(&runner),
        Some(
            &serde_json::json!({
                "captures": [
                    {"capture": c_base, "tree": "base", "viewport": "1440x900", "theme": ""},
                    {"capture": c_fix, "tree": "fix", "viewport": "1440x900", "theme": ""},
                    {"capture": c_app, "tree": "approved", "viewport": "1440x900", "theme": ""},
                    {"capture": c_app_fix, "tree": "approved-fix", "viewport": "1440x900", "theme": ""}
                ]
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(status(&done), StatusCode::OK, "{done}");

    let b_after: rupicola_api::Fix = json(
        &call(
            port,
            "GET",
            &format!("/v1/fixes/{}", fix_b.id),
            Some(&ana),
            None,
        )
        .await,
    );
    assert_eq!(b_after.comparisons.len(), 2);
    assert_eq!(
        b_after.comparisons[0].view,
        rupicola_api::ComparisonView::Isolated
    );
    assert_eq!(b_after.comparisons[0].changed_ratio, Some(1.0));
    assert!(b_after.comparisons[0].diff.is_some());
    assert_eq!(b_after.selections.len(), 2);

    // Approve Fix A -> Fix B's integrated comparison becomes stale and a recapture job is queued.
    let approved_a = call(
        port,
        "POST",
        &format!("/v1/fixes/{}/reviews", fix_a.id),
        Some(&ana),
        Some(r#"{"outcome":"approved","comment":"looks good"}"#),
    )
    .await;
    assert_eq!(status(&approved_a), StatusCode::OK, "{approved_a}");

    let b_stale: rupicola_api::Fix = json(
        &call(
            port,
            "GET",
            &format!("/v1/fixes/{}", fix_b.id),
            Some(&ana),
            None,
        )
        .await,
    );
    let integrated = b_stale
        .comparisons
        .iter()
        .find(|c| c.view == rupicola_api::ComparisonView::Integrated)
        .unwrap();
    assert!(integrated.stale);

    let open_jobs: Vec<Job> = json(
        &call(
            port,
            "GET",
            "/v1/projects/demo/jobs?open=true",
            Some(&ana),
            None,
        )
        .await,
    );
    assert_eq!(open_jobs.len(), 1);
    let recapture = &open_jobs[0];
    assert_eq!(recapture.fix.as_deref(), Some(fix_b.id.as_str()));
    assert_eq!(recapture.trees.len(), 2);
    assert_eq!(recapture.trees[0].fixes, vec![fix_a.id.clone()]);

    // Runner claims and completes the recapture job.
    let claimed2 = call(
        port,
        "POST",
        "/v1/projects/demo/jobs/claim",
        Some(&runner),
        None,
    )
    .await;
    assert_eq!(status(&claimed2), StatusCode::OK);
    let c_app2 = upload_png_capture(port, &runner, "rev-app2", [20, 20, 20, 255]).await;
    let c_app_fix2 = upload_png_capture(port, &runner, "rev-app-fix2", [210, 20, 20, 255]).await;
    let done2 = call(
        port,
        "POST",
        &format!("/v1/jobs/{}/complete", recapture.id),
        Some(&runner),
        Some(
            &serde_json::json!({
                "captures": [
                    {"capture": c_app2, "tree": "approved", "viewport": "1440x900", "theme": ""},
                    {"capture": c_app_fix2, "tree": "approved-fix", "viewport": "1440x900", "theme": ""}
                ]
            })
            .to_string(),
        ),
    )
    .await;
    assert_eq!(status(&done2), StatusCode::OK, "{done2}");

    let b_fresh: rupicola_api::Fix = json(
        &call(
            port,
            "GET",
            &format!("/v1/fixes/{}", fix_b.id),
            Some(&ana),
            None,
        )
        .await,
    );
    assert_eq!(b_fresh.comparisons.len(), 2);
    let fresh_int = b_fresh
        .comparisons
        .iter()
        .find(|c| c.view == rupicola_api::ComparisonView::Integrated)
        .unwrap();
    assert!(!fresh_int.stale);
    assert_eq!(fresh_int.applied_fixes, vec![fix_a.id.clone()]);

    // Turning off auto_recapture prevents new recapture jobs on approval.
    let off = call(
        port,
        "PATCH",
        "/v1/projects/demo/capture-settings",
        Some(&ana),
        Some(r#"{"auto_recapture":false}"#),
    )
    .await;
    assert_eq!(status(&off), StatusCode::OK, "{off}");
    assert!(!json::<rupicola_api::RecaptureSettings>(&off).auto_recapture);

    let fix_c: rupicola_api::Fix = json(
        &call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&format!(
                r#"{{"title":"Fix C","git_ref":"fix/c","problems":["{prob_id}"]}}"#
            )),
        )
        .await,
    );
    let approved_c = call(
        port,
        "POST",
        &format!("/v1/fixes/{}/reviews", fix_c.id),
        Some(&ana),
        Some(r#"{"outcome":"approved"}"#),
    )
    .await;
    assert_eq!(status(&approved_c), StatusCode::OK, "{approved_c}");

    let open_after: Vec<Job> = json(
        &call(
            port,
            "GET",
            "/v1/projects/demo/jobs?open=true",
            Some(&ana),
            None,
        )
        .await,
    );
    assert_eq!(open_after.len(), 0);
}
