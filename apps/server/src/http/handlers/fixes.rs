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
        let response = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(token),
            Some(r#"{"title":"Cramped","severity":"minor","origin":"user-reported"}"#),
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
}
