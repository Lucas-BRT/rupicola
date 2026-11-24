//! `/projects/{id}/problems` and `/problems/{id}` handlers.

use crate::{
    domain::{ApiError, Principal},
    services::problems,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{Comment, NewComment, NewProblem, Problem, ProblemPatch, ProblemSummary};

/// `POST /v1/projects/{id}/problems`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewProblem>,
) -> Result<(StatusCode, Json<Problem>), ApiError> {
    problems::create(&state, &principal, &project, &form)
        .await
        .map(|problem| (StatusCode::CREATED, Json(problem)))
}

/// `GET /v1/projects/{id}/problems`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<ProblemSummary>>, ApiError> {
    problems::list(&state, &principal, &project).await.map(Json)
}

/// `GET /v1/projects/{id}/problems/details`
pub async fn list_details(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<Problem>>, ApiError> {
    problems::list_details(&state, &principal, &project)
        .await
        .map(Json)
}

/// `POST /v1/projects/{id}/captures/{capture}/analyze`
pub async fn analyze_capture(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, capture)): Path<(String, String)>,
) -> Result<Json<Vec<Problem>>, ApiError> {
    problems::analyze_capture(&state, &principal, &project, &capture)
        .await
        .map(Json)
}

/// `GET /v1/problems/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Problem>, ApiError> {
    problems::get(&state, &principal, &id).await.map(Json)
}

/// `PATCH /v1/problems/{id}`
pub async fn update(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(patch): Json<ProblemPatch>,
) -> Result<Json<Problem>, ApiError> {
    problems::update(&state, &principal, &id, &patch)
        .await
        .map(Json)
}

/// `POST /v1/problems/{id}/comments`
pub async fn comment(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<NewComment>,
) -> Result<(StatusCode, Json<Comment>), ApiError> {
    problems::comment(&state, &principal, &id, &form)
        .await
        .map(|comment| (StatusCode::CREATED, Json(comment)))
}

/// `PUT /v1/problems/{id}/selections/{selection}`: the problem cites one more selection.
pub async fn cite_selection(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, selection)): Path<(String, String)>,
) -> Result<Json<Problem>, ApiError> {
    problems::cite_selection(&state, &principal, &id, &selection)
        .await
        .map(Json)
}

/// `DELETE /v1/problems/{id}/selections/{selection}`: the problem stops citing a selection,
/// which stays.
pub async fn uncite_selection(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, selection)): Path<(String, String)>,
) -> Result<Json<Problem>, ApiError> {
    problems::uncite_selection(&state, &principal, &id, &selection)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Comment, Problem, ProblemStatus, ProblemSummary, Severity};

    fn new_problem(evidence: &str) -> String {
        format!(
            r#"{{"title":"Low contrast","severity":"major","origin":"agent-measured",
                "route":"/home","diagnosis":"ratio 2.1",
                "rules":[{{"scope":"core","code":"contrast","title":"Contrast"}}],
                "evidence":[{evidence}]}}"#
        )
    }

    #[tokio::test]
    async fn a_problem_is_filed_triaged_and_discussed() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(&ana),
            Some(&new_problem(&format!("\"{PROJECT_ARTIFACT}\""))),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let problem: Problem = json(&created);
        assert_eq!(problem.status, ProblemStatus::Open);
        assert_eq!(problem.evidence, [PROJECT_ARTIFACT]);
        assert_eq!(problem.rules[0].code, "contrast");

        let path = format!("/v1/problems/{}", problem.id);
        let triaged = call(
            port,
            "PATCH",
            &path,
            Some(&ana),
            Some(r#"{"status":"in-progress","severity":"critical"}"#),
        )
        .await;
        assert_eq!(status(&triaged), StatusCode::OK, "{triaged}");
        let triaged: Problem = json(&triaged);
        assert_eq!(triaged.status, ProblemStatus::InProgress);
        assert_eq!(triaged.severity, Severity::Critical);
        assert_eq!(triaged.diagnosis, "ratio 2.1");

        let comment = call(
            port,
            "POST",
            &format!("{path}/comments"),
            Some(&ana),
            Some(r#"{"text":"on it"}"#),
        )
        .await;
        assert_eq!(status(&comment), StatusCode::CREATED, "{comment}");
        assert_eq!(json::<Comment>(&comment).author.as_deref(), Some("ana"));

        let listed: Vec<ProblemSummary> = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/problems",
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, ProblemStatus::InProgress);
        let read: Problem = json(&call(port, "GET", &path, Some(&bruno), None).await);
        assert_eq!(read.comments.len(), 1);
    }

    #[tokio::test]
    async fn readers_cannot_change_and_outsiders_cannot_see() {
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
        let denied = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(&bruno),
            Some(&new_problem("")),
        )
        .await;
        assert_eq!(status(&denied), StatusCode::FORBIDDEN, "{denied}");
        let created: Problem = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/problems",
                Some(&ana),
                Some(&new_problem("")),
            )
            .await,
        );
        let path = format!("/v1/problems/{}", created.id);
        let patched = call(
            port,
            "PATCH",
            &path,
            Some(&bruno),
            Some(r#"{"status":"resolved"}"#),
        )
        .await;
        assert_eq!(status(&patched), StatusCode::FORBIDDEN, "{patched}");
        let hidden = call(port, "GET", &path, Some(&carla), None).await;
        assert_eq!(status(&hidden), StatusCode::NOT_FOUND, "{hidden}");
    }

    #[tokio::test]
    async fn evidence_must_belong_to_the_project() {
        let (port, [ana, ..]) = demo_with_artifacts().await;
        let response = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(&ana),
            Some(&new_problem(&format!("\"{FOREIGN_ARTIFACT}\""))),
        )
        .await;
        assert_eq!(
            status(&response),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{response}"
        );
    }

    const PROJECT_CAPTURE: &str = "capture-shot";

    #[tokio::test]
    async fn analyzing_a_capture_infers_problems_and_respects_suppressions() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        // Findingless capture falls back to a NeedsReprocess visual-inspection problem.
        let inferred: Vec<Problem> = json(
            &call(
                port,
                "POST",
                &format!("/v1/projects/demo/captures/{PROJECT_CAPTURE}/analyze"),
                Some(&ana),
                None,
            )
            .await,
        );
        assert_eq!(inferred.len(), 1);
        assert_eq!(inferred[0].status, ProblemStatus::NeedsReprocess);
        // Running analyze again deduplicates by diagnosis.
        let repeated: Vec<Problem> = json(
            &call(
                port,
                "POST",
                &format!("/v1/projects/demo/captures/{PROJECT_CAPTURE}/analyze"),
                Some(&ana),
                None,
            )
            .await,
        );
        assert_eq!(repeated, Vec::<Problem>::new());

        // Add findings and verify rule inference + suppression by intentional problem.
        call(
            port,
            "POST",
            &format!("/v1/captures/{PROJECT_CAPTURE}/findings"),
            Some(&ana),
            Some(&format!(
                r#"{{"note":"[/settings @ 320x844 light] Contrast too low","evidence_artifact_id":"{PROJECT_ARTIFACT}"}}"#
            )),
        )
        .await;
        let analyzed: Vec<Problem> = json(
            &call(
                port,
                "POST",
                &format!("/v1/projects/demo/captures/{PROJECT_CAPTURE}/analyze"),
                Some(&ana),
                None,
            )
            .await,
        );
        assert_eq!(analyzed.len(), 1);
        assert_eq!(analyzed[0].severity, Severity::Critical);
        assert_eq!(analyzed[0].route, "/settings");

        // Readers cannot analyze captures.
        let denied = call(
            port,
            "POST",
            &format!("/v1/projects/demo/captures/{PROJECT_CAPTURE}/analyze"),
            Some(&bruno),
            None,
        )
        .await;
        assert_eq!(status(&denied), StatusCode::FORBIDDEN, "{denied}");

        // Detailed list returns all problems with rules and evidence.
        let detailed: Vec<Problem> = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/problems/details",
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(detailed.len(), 2);
    }
}
