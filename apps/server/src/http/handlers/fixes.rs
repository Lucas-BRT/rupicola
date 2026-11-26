//! `/projects/{id}/fixes` and `/fixes/{id}` handlers.

use crate::{
    domain::{ApiError, Principal},
    services::fixes,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{Fix, FixSelectionLink, FixSummary, NewFix, NewReview, NewStep, Step};

/// `POST /v1/projects/{id}/fixes`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewFix>,
) -> Result<(StatusCode, Json<Fix>), ApiError> {
    fixes::create(&state, &principal, &project, &form)
        .await
        .map(|fix| (StatusCode::CREATED, Json(fix)))
}

/// `GET /v1/projects/{id}/fixes`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<FixSummary>>, ApiError> {
    fixes::list(&state, &principal, &project).await.map(Json)
}

/// `GET /v1/projects/{id}/fixes/details`
pub async fn list_details(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<Fix>>, ApiError> {
    fixes::list_details(&state, &principal, &project)
        .await
        .map(Json)
}

/// `GET /v1/fixes/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Fix>, ApiError> {
    fixes::get(&state, &principal, &id).await.map(Json)
}

/// `POST /v1/fixes/{id}/steps`: records one more verified step of the work.
pub async fn add_step(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(step): Json<NewStep>,
) -> Result<(StatusCode, Json<Step>), ApiError> {
    fixes::add_step(&state, &principal, &id, step)
        .await
        .map(|step| (StatusCode::CREATED, Json(step)))
}

/// `POST /v1/fixes/{id}/reviews`
pub async fn review(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<NewReview>,
) -> Result<Json<Fix>, ApiError> {
    fixes::review(&state, &principal, &id, &form)
        .await
        .map(Json)
}

/// `PUT /v1/fixes/{id}/selections/{selection}`: the fix cites a selection of the same project
/// as its baseline or as proof that it works.
pub async fn cite_selection(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, selection)): Path<(String, String)>,
    Json(link): Json<FixSelectionLink>,
) -> Result<Json<Fix>, ApiError> {
    fixes::cite_selection(&state, &principal, &id, &selection, &link)
        .await
        .map(Json)
}

/// `DELETE /v1/fixes/{id}/selections/{selection}`: the fix stops citing a selection, which
/// stays.
pub async fn uncite_selection(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, selection)): Path<(String, String)>,
) -> Result<Json<Fix>, ApiError> {
    fixes::uncite_selection(&state, &principal, &id, &selection)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{
        Fix, FixSelectionRole, FixStatus, FixSummary, Problem, ProblemStatus, Selection,
    };

    async fn file_problem(port: u16, token: &str) -> String {
        let body = format!(
            r#"{{"title":"Cramped","severity":"minor","origin":"user-reported","evidence":["{PROJECT_ARTIFACT}"]}}"#
        );
        let response = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(token),
            Some(&body),
        )
        .await;
        assert_eq!(status(&response), StatusCode::CREATED, "{response}");
        json::<Problem>(&response).id
    }

    fn new_fix(problem: &str, before: &str) -> String {
        format!(
            r#"{{"title":"Add spacing","summary":"8px grid","problems":["{problem}"],
                "tactics":["spacing"],"files":["src/ui.rs"],
                "comparisons":[{{"label":"home","before":"{before}","after":"{PROJECT_ARTIFACT}"}}],
                "steps":[{{"title":"measure","verification":"gap is 8px"}}]}}"#
        )
    }

    async fn problem_status(port: u16, token: &str, id: &str) -> ProblemStatus {
        json::<Problem>(
            &call(
                port,
                "GET",
                &format!("/v1/problems/{id}"),
                Some(token),
                None,
            )
            .await,
        )
        .status
    }

    #[tokio::test]
    async fn a_fix_moves_its_problems_through_review() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&new_fix(&problem, PROJECT_ARTIFACT)),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let fix: Fix = json(&created);
        assert_eq!(fix.status, FixStatus::PendingReview);
        assert_eq!(fix.comparisons.len(), 1);
        assert_eq!(fix.steps[0].step.title, "measure");
        assert_eq!(
            problem_status(port, &ana, &problem).await,
            ProblemStatus::InProgress
        );

        let path = format!("/v1/fixes/{}", fix.id);
        let step = call(
            port,
            "POST",
            &format!("{path}/steps"),
            Some(&ana),
            Some(r#"{"title":"verify on device"}"#),
        )
        .await;
        assert_eq!(status(&step), StatusCode::CREATED, "{step}");

        let changes = call(
            port,
            "POST",
            &format!("{path}/reviews"),
            Some(&ana),
            Some(r#"{"outcome":"changes-requested","comment":"more room"}"#),
        )
        .await;
        assert_eq!(json::<Fix>(&changes).status, FixStatus::ChangesRequested);
        assert_eq!(
            problem_status(port, &ana, &problem).await,
            ProblemStatus::InProgress
        );

        let approved = call(
            port,
            "POST",
            &format!("{path}/reviews"),
            Some(&ana),
            Some(r#"{"outcome":"approved"}"#),
        )
        .await;
        let approved: Fix = json(&approved);
        assert_eq!(approved.status, FixStatus::Approved);
        assert_eq!(approved.reviews.len(), 2);
        assert_eq!(approved.steps.len(), 2);
        assert_eq!(
            problem_status(port, &ana, &problem).await,
            ProblemStatus::Resolved
        );

        let listed: Vec<FixSummary> =
            json(&call(port, "GET", "/v1/projects/demo/fixes", Some(&bruno), None).await);
        assert_eq!((listed.len(), listed[0].problems), (1, 1));
    }

    #[tokio::test]
    async fn fixes_are_checked_against_their_project_and_roles() {
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let foreign = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&new_fix(&problem, FOREIGN_ARTIFACT)),
        )
        .await;
        assert_eq!(
            status(&foreign),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{foreign}"
        );
        let unknown = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&new_fix("missing", PROJECT_ARTIFACT)),
        )
        .await;
        assert_eq!(
            status(&unknown),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{unknown}"
        );
        let reader = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&bruno),
            Some(&new_fix(&problem, PROJECT_ARTIFACT)),
        )
        .await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN, "{reader}");
        let fix: Fix = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/fixes",
                Some(&ana),
                Some(&new_fix(&problem, PROJECT_ARTIFACT)),
            )
            .await,
        );
        let path = format!("/v1/fixes/{}", fix.id);
        let hidden = call(port, "GET", &path, Some(&carla), None).await;
        assert_eq!(status(&hidden), StatusCode::NOT_FOUND, "{hidden}");
        let pending = call(
            port,
            "POST",
            &format!("{path}/reviews"),
            Some(&ana),
            Some(r#"{"outcome":"pending-review"}"#),
        )
        .await;
        assert_eq!(
            status(&pending),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{pending}"
        );
    }

    /// The capture that holds the demo project's artifact.
    const PROJECT_CAPTURE: &str = "capture-shot";

    #[tokio::test]
    async fn a_fixs_comparisons_become_its_baseline_and_proof_and_it_can_bring_more() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let form = format!(
            r#"{{"title":"Add spacing","problems":["{problem}"],
                "comparisons":[{{"label":"home","before":"{PROJECT_ARTIFACT}","after":"{PROJECT_ARTIFACT}"}}],
                "proof":[{{"label":"Whole capture","items":[{{"capture":"{PROJECT_CAPTURE}","mark":"context"}}]}}]}}"#
        );
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&form),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let fix: Fix = json(&created);
        let cited: Vec<(FixSelectionRole, &str)> = fix
            .selections
            .iter()
            .map(|s| (s.role, s.selection.label.as_str()))
            .collect();
        assert_eq!(
            cited,
            [
                (FixSelectionRole::Baseline, "Before"),
                (FixSelectionRole::SolutionProof, "After"),
                (FixSelectionRole::SolutionProof, "Whole capture"),
            ]
        );
        assert_eq!(
            fix.comparisons.len(),
            1,
            "the comparison still pairs the files"
        );
    }

    /// A fix with one comparison of `view`, listing `applied` as the fixes already in its tree.
    fn viewed_fix(problem: &str, view: &str, applied: &[&str]) -> String {
        let applied = applied
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"title":"Add spacing","problems":["{problem}"],
                "comparisons":[{{"label":"home","view":"{view}","applied_fixes":[{applied}],
                  "base_revision":"a1b2c3d","revision":"e4f5a6b",
                  "before":"{PROJECT_ARTIFACT}","after":"{PROJECT_ARTIFACT}"}}]}}"#
        )
    }

    async fn file_fix(port: u16, token: &str, form: &str) -> Fix {
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(token),
            Some(form),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        json(&created)
    }

    async fn fetch(port: u16, token: &str, id: &str) -> Fix {
        json(&call(port, "GET", &format!("/v1/fixes/{id}"), Some(token), None).await)
    }

    #[tokio::test]
    async fn an_integrated_comparison_goes_stale_when_the_approved_fixes_change() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let first = file_fix(port, &ana, &new_fix(&problem, PROJECT_ARTIFACT)).await;
        let isolated = file_fix(port, &ana, &viewed_fix(&problem, "isolated", &[])).await;
        let alone = file_fix(port, &ana, &viewed_fix(&problem, "integrated", &[])).await;
        let with_first = file_fix(
            port,
            &ana,
            &viewed_fix(&problem, "integrated", &[first.id.as_str()]),
        )
        .await;
        assert!(!fetch(port, &ana, &alone.id).await.comparisons[0].stale);

        let approved = call(
            port,
            "POST",
            &format!("/v1/fixes/{}/reviews", first.id),
            Some(&ana),
            Some(r#"{"outcome":"approved"}"#),
        )
        .await;
        assert_eq!(status(&approved), StatusCode::OK, "{approved}");

        let alone = fetch(port, &ana, &alone.id).await;
        assert!(
            alone.comparisons[0].stale,
            "taken before the first fix was approved"
        );
        assert_eq!(alone.comparisons[0].revision, "e4f5a6b");
        assert!(!fetch(port, &ana, &with_first.id).await.comparisons[0].stale);
        assert!(
            !fetch(port, &ana, &isolated.id).await.comparisons[0].stale,
            "an isolated comparison does not depend on other fixes"
        );
        let listed: Vec<Fix> = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/fixes/details",
                Some(&ana),
                None,
            )
            .await,
        );
        let stale: Vec<&str> = listed
            .iter()
            .filter(|fix| fix.comparisons.iter().any(|c| c.stale))
            .map(|fix| fix.id.as_str())
            .collect();
        assert_eq!(stale, [alone.id.as_str()], "the detailed list agrees");
    }

    #[tokio::test]
    async fn only_isolated_comparisons_are_the_fixs_own_proof() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let form = format!(
            r#"{{"title":"Add spacing","problems":["{problem}"],
                "comparisons":[
                  {{"label":"alone","view":"isolated","before":"{PROJECT_ARTIFACT}","after":"{PROJECT_ARTIFACT}"}},
                  {{"label":"stacked","view":"integrated","before":"{PROJECT_ARTIFACT}","after":"{PROJECT_ARTIFACT}"}}]}}"#
        );
        let fix = file_fix(port, &ana, &form).await;
        assert_eq!(fix.comparisons.len(), 2);
        let sizes: Vec<usize> = fix
            .selections
            .iter()
            .map(|s| s.selection.items.len())
            .collect();
        assert_eq!(
            sizes,
            [1, 1],
            "one before and one after, from the isolated pair"
        );
    }

    #[tokio::test]
    async fn an_isolated_comparison_cannot_apply_other_fixes() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let refused = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&viewed_fix(&problem, "isolated", &["other"])),
        )
        .await;
        assert_eq!(
            status(&refused),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused}"
        );
    }

    #[tokio::test]
    async fn a_fix_cites_a_selection_changes_its_role_and_drops_it() {
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
        let problem = file_problem(port, &ana).await;
        let fix: Fix = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/fixes",
                Some(&ana),
                Some(&format!(r#"{{"title":"Tidy","problems":["{problem}"]}}"#)),
            )
            .await,
        );
        assert_eq!(fix.selections.len(), 0);
        let selection: Selection = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/selections",
                Some(&ana),
                Some(&format!(
                    r#"{{"label":"Before the fix","items":[{{"capture":"{PROJECT_CAPTURE}","mark":"proof"}}]}}"#
                )),
            )
            .await,
        );
        let path = format!("/v1/fixes/{}/selections/{}", fix.id, selection.id);
        let as_role = |role: &str| format!(r#"{{"role":"{role}"}}"#);
        let cited: Fix =
            json(&call(port, "PUT", &path, Some(&ana), Some(&as_role("baseline"))).await);
        assert_eq!(cited.selections[0].role, FixSelectionRole::Baseline);
        let moved: Fix = json(
            &call(
                port,
                "PUT",
                &path,
                Some(&ana),
                Some(&as_role("solution-proof")),
            )
            .await,
        );
        assert_eq!(
            moved.selections.len(),
            1,
            "citing again changes the role, not the count"
        );
        assert_eq!(moved.selections[0].role, FixSelectionRole::SolutionProof);
        let reader = call(port, "PUT", &path, Some(&bruno), Some(&as_role("baseline"))).await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
        let dropped: Fix = json(&call(port, "DELETE", &path, Some(&ana), None).await);
        assert_eq!(dropped.selections.len(), 0);
        let elsewhere = format!("/v1/fixes/{}/selections/not-a-selection", fix.id);
        let missing = call(
            port,
            "PUT",
            &elsewhere,
            Some(&ana),
            Some(&as_role("baseline")),
        )
        .await;
        assert_eq!(status(&missing), StatusCode::NOT_FOUND);
        let outsider = call(port, "PUT", &path, Some(&carla), Some(&as_role("baseline"))).await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND);
    }

    async fn publish_with_spec(
        port: u16,
        token: &str,
        id: &str,
        spec: Option<rupicola_api::CapturePlanSpec>,
    ) -> rupicola_api::Capture {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        let sha = stream_blob(port, token, "demo", png).await;
        let mut rec = capture_record(id, &sha, png.len() as u64, vec![]);
        rec.plan_spec = spec;
        let resp = publish_record(port, token, &rec).await;
        assert_eq!(status(&resp), StatusCode::CREATED, "{resp}");
        json(&resp)
    }

    fn spec(url_path: &str, zoom_milli: u32) -> rupicola_api::CapturePlanSpec {
        rupicola_api::CapturePlanSpec {
            url_path: url_path.to_owned(),
            viewport: [1280, 720],
            scale_milli: 2000,
            zoom_milli,
            color_scheme: Some("dark".to_owned()),
            reduced_motion: false,
            locale: None,
            timezone: None,
            steps: vec![rupicola_api::CaptureStep::Checkpoint {
                id: "home".to_owned(),
                full_page: false,
            }],
            scenario_name: Some("Home".to_owned()),
            scenario_hash: None,
        }
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn parity_guard_rejects_mismatched_url_zoom_and_baseline_and_accepts_matching_plan() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let base_cap = publish_with_spec(
            port,
            &ana,
            "01936f2c-1111-7bbb-8ccc-000000000001",
            Some(spec("/settings", 1000)),
        )
        .await;
        let matching_after = publish_with_spec(
            port,
            &ana,
            "01936f2c-1111-7bbb-8ccc-000000000002",
            Some(spec("http://preview.local:9000/settings", 1000)),
        )
        .await;
        let wrong_zoom = publish_with_spec(
            port,
            &ana,
            "01936f2c-1111-7bbb-8ccc-000000000003",
            Some(spec("/settings", 1250)),
        )
        .await;
        let wrong_page = publish_with_spec(
            port,
            &ana,
            "01936f2c-1111-7bbb-8ccc-000000000004",
            Some(spec("/projects", 1000)),
        )
        .await;
        let no_plan =
            publish_with_spec(port, &ana, "01936f2c-1111-7bbb-8ccc-000000000005", None).await;

        let problem: Problem = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/problems",
                Some(&ana),
                Some(&format!(
                    r#"{{"title":"Contrast","severity":"major","origin":"agent-visual","evidence":["{}"]}}"#,
                    base_cap.checkpoints[0].artifact.id
                )),
            )
            .await,
        );

        let fix_body = |before: &str, after: &str| {
            format!(
                r#"{{"title":"Fix","problems":["{}"],"comparisons":[{{"label":"main","before":"{before}","after":"{after}"}}]}}"#,
                problem.id
            )
        };

        let r_zoom = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&fix_body(
                &base_cap.checkpoints[0].artifact.id,
                &wrong_zoom.checkpoints[0].artifact.id,
            )),
        )
        .await;
        assert_eq!(
            status(&r_zoom),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{r_zoom}"
        );
        assert!(r_zoom.contains("zoom_milli"), "{r_zoom}");

        let r_page = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&fix_body(
                &base_cap.checkpoints[0].artifact.id,
                &wrong_page.checkpoints[0].artifact.id,
            )),
        )
        .await;
        assert_eq!(
            status(&r_page),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{r_page}"
        );
        assert!(r_page.contains("url_path"), "{r_page}");

        let r_no_plan = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&fix_body(
                &base_cap.checkpoints[0].artifact.id,
                &no_plan.checkpoints[0].artifact.id,
            )),
        )
        .await;
        assert_eq!(
            status(&r_no_plan),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{r_no_plan}"
        );

        // Baseline from unrelated page (even if before==after) fails Problem <-> Baseline check
        let r_wrong_base = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&fix_body(
                &wrong_page.checkpoints[0].artifact.id,
                &wrong_page.checkpoints[0].artifact.id,
            )),
        )
        .await;
        assert_eq!(
            status(&r_wrong_base),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{r_wrong_base}"
        );

        let r_ok = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&fix_body(
                &base_cap.checkpoints[0].artifact.id,
                &matching_after.checkpoints[0].artifact.id,
            )),
        )
        .await;
        assert_eq!(status(&r_ok), StatusCode::CREATED, "{r_ok}");
    }

    #[tokio::test]
    async fn mismatch_override_is_human_only_by_default_and_configurable_per_project() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let base_cap = publish_with_spec(
            port,
            &ana,
            "01936f2c-2222-7bbb-8ccc-000000000001",
            Some(spec("/old-route", 1000)),
        )
        .await;
        let new_route_cap = publish_with_spec(
            port,
            &ana,
            "01936f2c-2222-7bbb-8ccc-000000000002",
            Some(spec("/new-route", 1000)),
        )
        .await;
        let problem: Problem = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/problems",
                Some(&ana),
                Some(&format!(
                    r#"{{"title":"Route moved","severity":"minor","origin":"agent-visual","evidence":["{}"]}}"#,
                    base_cap.checkpoints[0].artifact.id
                )),
            )
            .await,
        );
        // Issue a machine token (AI agent) for Ana
        let issued: rupicola_api::IssuedCredential = json(
            &call(
                port,
                "POST",
                "/v1/tokens",
                Some(&ana),
                Some(r#"{"label":"ai-agent"}"#),
            )
            .await,
        );
        let machine = issued.token;
        let override_body = format!(
            r#"{{"title":"Move route","problems":["{}"],"comparisons":[{{"label":"main","before":"{}","after":"{}"}}],"mismatch_override_reason":"Intentional route rename"}}"#,
            problem.id,
            base_cap.checkpoints[0].artifact.id,
            new_route_cap.checkpoints[0].artifact.id
        );

        // Machine token is forbidden from overriding by default
        let r_machine_blocked = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&machine),
            Some(&override_body),
        )
        .await;
        assert_eq!(
            status(&r_machine_blocked),
            StatusCode::FORBIDDEN,
            "{r_machine_blocked}"
        );

        // Human session can override by default and reason is persisted
        let r_human = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&ana),
            Some(&override_body),
        )
        .await;
        assert_eq!(status(&r_human), StatusCode::CREATED, "{r_human}");
        let created: Fix = json(&r_human);
        assert_eq!(
            created.mismatch_override_reason.as_deref(),
            Some("Intentional route rename")
        );

        // Owner enables allow_agent_mismatch_override for the project -> machine token now allowed
        let r_cfg = call(
            port,
            "PATCH",
            "/v1/projects/demo/capture-settings",
            Some(&ana),
            Some(r#"{"allow_agent_mismatch_override":true}"#),
        )
        .await;
        assert_eq!(status(&r_cfg), StatusCode::OK, "{r_cfg}");

        let r_machine_ok = call(
            port,
            "POST",
            "/v1/projects/demo/fixes",
            Some(&machine),
            Some(&override_body),
        )
        .await;
        assert_eq!(status(&r_machine_ok), StatusCode::CREATED, "{r_machine_ok}");
    }
}
