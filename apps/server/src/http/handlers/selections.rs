//! `/projects/{id}/selections` and `/selections/{id}` handlers.

use crate::{
    domain::{ApiError, Principal},
    services::selections,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{NewSelection, Selection};

/// `POST /v1/projects/{id}/selections`: keeps a selection to cite later.
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewSelection>,
) -> Result<(StatusCode, Json<Selection>), ApiError> {
    selections::create(&state, &principal, &project, &form)
        .await
        .map(|selection| (StatusCode::CREATED, Json(selection)))
}

/// `GET /v1/selections/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Selection>, ApiError> {
    selections::get(&state, &principal, &id).await.map(Json)
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Capture, Problem, Selection, SelectionMark};
    use serde_json::json;

    const A: &str = "01936f2c-aaaa-7bbb-8ccc-0000000005a1";
    const B: &str = "01936f2c-aaaa-7bbb-8ccc-0000000005b1";
    const PATH: &str = "checkpoints/home.png";

    /// Ana owns `demo` with captures A and B; Bruno reads; Carla owns `other` with a capture.
    /// Returns the tokens and the artifact id of A's file.
    async fn demo() -> (u16, [String; 3], String) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "carla"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno, carla] = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "carla").await,
        ];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        let sha = stream_blob(port, &ana, "demo", b"pixels").await;
        for id in [A, B] {
            publish_record(port, &ana, &capture_record(id, &sha, 6, Vec::new())).await;
        }
        let capture: Capture =
            json(&call(port, "GET", &format!("/v1/captures/{A}"), Some(&ana), None).await);
        let artifact = capture.checkpoints[0].artifact.id.clone();
        (port, [ana, bruno, carla], artifact)
    }

    async fn select(port: u16, token: &str, body: &serde_json::Value) -> String {
        call(
            port,
            "POST",
            "/v1/projects/demo/selections",
            Some(token),
            Some(&body.to_string()),
        )
        .await
    }

    #[tokio::test]
    async fn a_selection_names_files_by_id_or_by_path_and_whole_captures() {
        let (port, [ana, bruno, _], artifact) = demo().await;
        let body = json!({
            "label": "Menu contrast",
            "items": [
                { "capture": A, "artifact": artifact, "mark": "proof", "fragment": { "rect": [0, 0, 40, 20] } },
                { "capture": B, "path": PATH, "mark": "proof" },
                { "capture": B, "mark": "context" }
            ]
        });
        let made = select(port, &ana, &body).await;
        assert_eq!(status(&made), StatusCode::CREATED, "{made}");
        let selection: Selection = json(&made);
        assert_eq!(selection.items.len(), 3);
        assert_eq!(
            selection.items[0].fragment.as_ref().unwrap().rect,
            Some([0, 0, 40, 20])
        );
        assert!(
            selection.items[1].artifact.is_some(),
            "a path comes back as the artifact id"
        );
        assert_eq!(selection.items[2].mark, SelectionMark::Context);
        assert_eq!(selection.proof().count(), 2);
        let read = call(
            port,
            "GET",
            &format!("/v1/selections/{}", selection.id),
            Some(&bruno),
            None,
        )
        .await;
        assert_eq!(json::<Selection>(&read), selection);
    }

    #[tokio::test]
    async fn an_anchor_outside_the_project_or_its_capture_is_refused() {
        let (port, [ana, bruno, _], artifact) = demo().await;
        for (form, why) in [
            (
                json!({ "label": "x", "items": [{ "capture": "nope", "mark": "proof" }] }),
                "not a capture",
            ),
            (
                json!({ "label": "x", "items": [{ "capture": B, "artifact": artifact, "mark": "proof" }] }),
                "has no file",
            ),
            (
                json!({ "label": "x", "items": [{ "capture": A, "path": "missing.png", "mark": "proof" }] }),
                "has no file",
            ),
            (
                json!({ "label": " ", "items": [{ "capture": A, "mark": "proof" }] }),
                "label",
            ),
            (json!({ "label": "x", "items": [] }), "anchors"),
        ] {
            let refused = select(port, &ana, &form).await;
            assert_eq!(
                status(&refused),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{refused}"
            );
            assert!(body(&refused).contains(why), "{why}: {refused}");
        }
        let reader = select(
            port,
            &bruno,
            &json!({ "label": "x", "items": [{ "capture": A, "mark": "proof" }] }),
        )
        .await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_problem_cites_its_evidence_and_more_selections_and_can_drop_one() {
        let (port, [ana, _, carla], artifact) = demo().await;
        let filed = call(
            port,
            "POST",
            "/v1/projects/demo/problems",
            Some(&ana),
            Some(
                &json!({
                    "title": "Low contrast", "severity": "major", "origin": "user-reported",
                    "evidence": [artifact],
                    "selections": [{ "label": "Mobile too", "items": [{ "capture": B, "path": PATH, "mark": "proof" }] }]
                })
                .to_string(),
            ),
        )
        .await;
        assert_eq!(status(&filed), StatusCode::CREATED, "{filed}");
        let problem: Problem = json(&filed);
        let labels: Vec<&str> = problem
            .selections
            .iter()
            .map(|s| s.label.as_str())
            .collect();
        assert_eq!(labels, ["Evidence", "Mobile too"]);
        assert_eq!(problem.evidence.len(), 2, "both files are proof");
        let extra: Selection = json(
            &select(
                port,
                &ana,
                &json!({ "label": "Context", "items": [{ "capture": A, "mark": "context" }] }),
            )
            .await,
        );
        let cited = call(
            port,
            "PUT",
            &format!("/v1/problems/{}/selections/{}", problem.id, extra.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(json::<Problem>(&cited).selections.len(), 3);
        let dropped = call(
            port,
            "DELETE",
            &format!(
                "/v1/problems/{}/selections/{}",
                problem.id, problem.selections[0].id
            ),
            Some(&ana),
            None,
        )
        .await;
        let after: Problem = json(&dropped);
        assert_eq!(after.selections.len(), 2);
        assert_eq!(
            after.evidence.len(),
            1,
            "the dropped selection's proof is gone"
        );
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&carla),
            Some(r#"{"id":"other","label":"Other"}"#),
        )
        .await;
        let foreign = call(
            port,
            "PUT",
            &format!("/v1/problems/{}/selections/nope", problem.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&foreign), StatusCode::NOT_FOUND);
    }
}
