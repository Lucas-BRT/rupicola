//! `/captures/{id}` removal, `/captures/{id}/citations` and `/projects/{id}/deletion-policy`.

use crate::{
    domain::{ApiError, Principal},
    services::removal,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{CaptureRemoval, Citations, PolicyView};

/// `GET /v1/captures/{id}/citations`: what cites the capture, the captures that descend from it,
/// and what removing it would take.
pub async fn citations(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
) -> Result<Json<Citations>, ApiError> {
    removal::citations(&state, &principal, &capture)
        .await
        .map(Json)
}

/// `DELETE /v1/captures/{id}`: removes the capture, leaving a tombstone. A cited capture needs
/// an owner, a policy that allows it and the confirmation the citations returned. An open
/// problem left with no live proof goes back to needs-reprocess.
pub async fn remove(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
    Json(form): Json<CaptureRemoval>,
) -> Result<StatusCode, ApiError> {
    removal::remove(&state, &principal, &capture, &form)
        .await
        .map(|()| StatusCode::NO_CONTENT)
}

/// `GET /v1/projects/{id}/deletion-policy`
pub async fn policy(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<PolicyView>, ApiError> {
    removal::policy(&state, &principal, &project)
        .await
        .map(Json)
}

/// `PUT /v1/projects/{id}/deletion-policy`: owners choose whether a cited capture may go.
pub async fn set_policy(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<PolicyView>,
) -> Result<Json<PolicyView>, ApiError> {
    removal::set_policy(&state, &principal, &project, &form)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::{state::Shared, testkit::*};
    use axum::http::StatusCode;
    use rupicola_api::{
        Capture, CaptureSummary, CitationKind, Citations, Problem, ProblemStatus,
        record::{ParentKind, ParentRef},
    };
    use serde_json::json;
    use std::sync::atomic::Ordering;

    const A: &str = "01936f2c-aaaa-7bbb-8ccc-0000000007a1";
    const B: &str = "01936f2c-aaaa-7bbb-8ccc-0000000007b1";
    const C: &str = "01936f2c-aaaa-7bbb-8ccc-0000000007c1";

    /// Ana owns `demo`, Bruno reviews and Dani reads. Captures A, B (a child of A) and C; a
    /// problem cites A's file as its only proof. Returns the problem's id too.
    async fn demo() -> (u16, Shared, [String; 3], String) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "dani"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno, dani] = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "dani").await,
        ];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        for (handle, role) in [("bruno", "reviewer"), ("dani", "reader")] {
            call(
                port,
                "PUT",
                &format!("/v1/projects/demo/members/{handle}"),
                Some(&ana),
                Some(&format!(r#"{{"role":"{role}"}}"#)),
            )
            .await;
        }
        let sha = stream_blob(port, &ana, "demo", b"pixels").await;
        let child_of_a = vec![ParentRef {
            id: A.to_owned(),
            kind: ParentKind::Previous,
        }];
        for (id, parents) in [(A, Vec::new()), (B, child_of_a), (C, Vec::new())] {
            publish_record(port, &ana, &capture_record(id, &sha, 6, parents)).await;
        }
        let capture: Capture =
            json(&call(port, "GET", &format!("/v1/captures/{A}"), Some(&ana), None).await);
        let problem: Problem = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/problems",
                Some(&ana),
                Some(
                    &json!({ "title": "Low contrast", "severity": "major", "origin": "user-reported",
                             "evidence": [capture.checkpoints[0].artifact.id] })
                    .to_string(),
                ),
            )
            .await,
        );
        (port, state, [ana, bruno, dani], problem.id)
    }

    async fn citations(port: u16, token: &str, capture: &str) -> Citations {
        json(
            &call(
                port,
                "GET",
                &format!("/v1/captures/{capture}/citations"),
                Some(token),
                None,
            )
            .await,
        )
    }

    async fn remove(port: u16, token: &str, capture: &str, confirm: Option<&str>) -> String {
        call(
            port,
            "DELETE",
            &format!("/v1/captures/{capture}"),
            Some(token),
            Some(&json!({ "reason": "wrong build", "confirm": confirm }).to_string()),
        )
        .await
    }

    #[tokio::test]
    async fn a_capture_nothing_cites_goes_with_a_reviewer_and_leaves_a_tombstone() {
        let (port, _, [_, bruno, dani], _) = demo().await;
        let read = citations(port, &bruno, C).await;
        assert!(read.citations.is_empty() && read.allowed && read.confirm.is_none());
        assert_eq!(
            status(&remove(port, &dani, C, None).await),
            StatusCode::FORBIDDEN,
            "a reader"
        );
        let removed = remove(port, &bruno, C, None).await;
        assert_eq!(status(&removed), StatusCode::NO_CONTENT, "{removed}");
        let tomb: Capture =
            json(&call(port, "GET", &format!("/v1/captures/{C}"), Some(&dani), None).await);
        let deleted = tomb.deleted.expect("a tombstone");
        assert_eq!(
            (deleted.by.as_str(), deleted.reason.as_str()),
            ("bruno", "wrong build")
        );
        let listed: Vec<CaptureSummary> =
            json(&call(port, "GET", "/v1/projects/demo/captures", Some(&dani), None).await);
        assert!(
            listed.iter().all(|capture| capture.id != C),
            "a removed capture is not listed"
        );
        assert_eq!(
            status(&remove(port, &bruno, C, None).await),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn a_cited_capture_goes_only_with_an_owner_who_confirms_and_its_problem_reopens() {
        let (port, state, [ana, bruno, _], problem) = demo().await;
        let read = citations(port, &bruno, A).await;
        assert_eq!(read.citations.len(), 1);
        assert_eq!(read.citations[0].kind, CitationKind::Problem);
        assert_eq!(read.children, [B], "a child is listed and does not block");
        assert!(!read.allowed, "a reviewer cannot remove a cited capture");
        assert_eq!(
            status(&remove(port, &bruno, A, read.confirm.as_deref()).await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&remove(port, &ana, A, None).await),
            StatusCode::CONFLICT
        );
        let owner = citations(port, &ana, A).await;
        assert!(owner.allowed);
        let removed = remove(port, &ana, A, owner.confirm.as_deref()).await;
        assert_eq!(status(&removed), StatusCode::NO_CONTENT, "{removed}");
        let reopened: Problem = json(
            &call(
                port,
                "GET",
                &format!("/v1/problems/{problem}"),
                Some(&ana),
                None,
            )
            .await,
        );
        assert_eq!(
            reopened.status,
            ProblemStatus::NeedsReprocess,
            "its only proof is gone"
        );
        assert_eq!(reopened.selections.len(), 1, "the citation still resolves");
        let detail: String = sqlx::query_scalar(
            "SELECT detail FROM audit_events WHERE event LIKE 'Removed capture %'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert!(
            detail.contains(&problem),
            "the audit keeps what cited it: {detail}"
        );
    }

    #[tokio::test]
    async fn a_confirmation_goes_stale_when_a_new_citation_appears() {
        let (port, _, [ana, ..], _) = demo().await;
        let seen = citations(port, &ana, A).await;
        call(
            port,
            "POST",
            "/v1/projects/demo/selections",
            Some(&ana),
            Some(
                &json!({ "label": "Later", "items": [{ "capture": A, "mark": "context" }] })
                    .to_string(),
            ),
        )
        .await;
        let stale = remove(port, &ana, A, seen.confirm.as_deref()).await;
        assert_eq!(status(&stale), StatusCode::CONFLICT, "{stale}");
        let now = citations(port, &ana, A).await;
        assert_eq!(now.citations.len(), 2);
        assert!(
            now.citations
                .iter()
                .any(|c| c.kind == CitationKind::Selection)
        );
    }

    #[tokio::test]
    async fn the_project_and_the_server_can_each_forbid_removing_cited_captures() {
        let (port, state, [ana, bruno, _], _) = demo().await;
        let reviewer = call(
            port,
            "PUT",
            "/v1/projects/demo/deletion-policy",
            Some(&bruno),
            Some(r#"{"policy":"forbid"}"#),
        )
        .await;
        assert_eq!(
            status(&reviewer),
            StatusCode::FORBIDDEN,
            "only owners choose"
        );
        call(
            port,
            "PUT",
            "/v1/projects/demo/deletion-policy",
            Some(&ana),
            Some(r#"{"policy":"forbid"}"#),
        )
        .await;
        let forbidden = citations(port, &ana, A).await;
        assert!(!forbidden.allowed);
        assert_eq!(
            status(&remove(port, &ana, A, forbidden.confirm.as_deref()).await),
            StatusCode::FORBIDDEN
        );
        call(
            port,
            "PUT",
            "/v1/projects/demo/deletion-policy",
            Some(&ana),
            Some(r#"{"policy":"confirm"}"#),
        )
        .await;
        state.cited_deletion.store(false, Ordering::Relaxed);
        let ceiling = citations(port, &ana, A).await;
        assert!(!ceiling.allowed, "the server's never wins over the project");
        assert_eq!(
            status(&remove(port, &ana, A, ceiling.confirm.as_deref()).await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&remove(port, &bruno, C, None).await),
            StatusCode::NO_CONTENT,
            "an uncited capture still goes"
        );
    }
}
