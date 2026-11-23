//! `PUT /projects/{id}/captures/{capture}`: publishing a capture record.

use crate::{
    domain::{ApiError, Principal},
    services::publish::{Published, publish},
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{Capture, record::CaptureRecord};

/// `PUT /v1/projects/{id}/captures/{capture}`: 201 when the record is new, 200 when the same
/// record was kept before, 409 when the id holds other content.
pub async fn put(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
    Json(record): Json<CaptureRecord>,
) -> Result<(StatusCode, Json<Capture>), ApiError> {
    Ok(
        match publish(&state, &principal, &project, &id, &record).await? {
            Published::New(capture) => (StatusCode::CREATED, Json(capture)),
            Published::Same(capture) => (StatusCode::OK, Json(capture)),
        },
    )
}

#[cfg(test)]
mod tests {
    use crate::{state::Shared, testkit::*};
    use axum::http::StatusCode;
    use rupicola_api::{
        Capture, CaptureStatus,
        record::{MemberRole, ParentKind, ParentRef},
    };

    const FIRST: &str = "01936f2c-aaaa-7bbb-8ccc-000000000001";
    const SECOND: &str = "01936f2c-aaaa-7bbb-8ccc-000000000002";
    const HOME: &[u8] = b"home pixels";

    /// Ana owns `demo`, where Bruno reads.
    async fn demo() -> (u16, Shared, [String; 2]) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno] = [sign_in(port, "ana").await, sign_in(port, "bruno").await];
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
        (port, state, [ana, bruno])
    }

    async fn count(state: &crate::state::AppState, sql: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&state.db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_record_is_kept_once_and_the_same_record_again_changes_nothing() {
        let (port, state, [ana, _]) = demo().await;
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        let first = capture_record(FIRST, &sha, 11, Vec::new());
        let created = publish_record(port, &ana, &first).await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let capture: Capture = json(&created);
        assert_eq!(capture.id, FIRST);
        assert_eq!(capture.checkpoints[0].artifact.sha256, sha);
        assert_eq!(
            capture.environment.width,
            Some(800),
            "the views read the older shape"
        );
        assert_eq!(capture.environment.scale, Some(2.0));
        let again = publish_record(port, &ana, &first).await;
        assert_eq!(status(&again), StatusCode::OK, "{again}");
        assert_eq!(count(&state, "SELECT count(*) FROM captures").await, 1);
        let surface = count(
            &state,
            "SELECT count(*) FROM artifacts a JOIN surfaces s ON s.id = a.surface_id
             WHERE s.key = 'home' AND a.locator = '{\"route\":\"/\"}'",
        )
        .await;
        assert_eq!(surface, 1, "the screen and its locator are kept");
        let facets = count(
            &state,
            "SELECT count(*) FROM capture_facets
             WHERE key = 'appearance.color_scheme' AND value = 'dark'",
        )
        .await;
        assert_eq!(facets, 1, "the environment can be searched");
    }

    #[tokio::test]
    async fn other_content_under_a_kept_id_is_a_conflict() {
        let (port, _, [ana, _]) = demo().await;
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        publish_record(port, &ana, &capture_record(FIRST, &sha, 11, Vec::new())).await;
        let mut changed = capture_record(FIRST, &sha, 11, Vec::new());
        changed.captured_at += 1;
        let response = publish_record(port, &ana, &changed).await;
        assert_eq!(status(&response), StatusCode::CONFLICT, "{response}");
    }

    #[tokio::test]
    async fn files_must_be_held_and_their_sizes_true() {
        let (port, _, [ana, _]) = demo().await;
        let absent = "2".repeat(64);
        let response =
            publish_record(port, &ana, &capture_record(FIRST, &absent, 11, Vec::new())).await;
        assert_eq!(
            status(&response),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{response}"
        );
        assert!(body(&response).contains(&absent), "{response}");
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        let wrong_size =
            publish_record(port, &ana, &capture_record(FIRST, &sha, 99, Vec::new())).await;
        assert_eq!(
            status(&wrong_size),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{wrong_size}"
        );
    }

    #[tokio::test]
    async fn a_parent_must_be_published_first_and_then_the_lineage_is_kept() {
        let (port, state, [ana, _]) = demo().await;
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        let parent = vec![ParentRef {
            id: FIRST.to_owned(),
            kind: ParentKind::Previous,
        }];
        let early = publish_record(
            port,
            &ana,
            &capture_record(SECOND, &sha, 11, parent.clone()),
        )
        .await;
        assert_eq!(status(&early), StatusCode::UNPROCESSABLE_ENTITY, "{early}");
        assert!(body(&early).contains(FIRST), "{early}");
        publish_record(port, &ana, &capture_record(FIRST, &sha, 11, Vec::new())).await;
        let child = publish_record(port, &ana, &capture_record(SECOND, &sha, 11, parent)).await;
        assert_eq!(status(&child), StatusCode::CREATED, "{child}");
        let edges = count(
            &state,
            "SELECT count(*) FROM capture_parents WHERE capture_id = '01936f2c-aaaa-7bbb-8ccc-000000000002'
               AND parent_id = '01936f2c-aaaa-7bbb-8ccc-000000000001' AND origin = 'record'",
        )
        .await;
        assert_eq!(edges, 1);
    }

    #[tokio::test]
    async fn a_record_that_does_not_match_its_address_or_rules_is_refused() {
        let (port, _, [ana, bruno]) = demo().await;
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        let first = capture_record(FIRST, &sha, 11, Vec::new());
        let elsewhere = call(
            port,
            "PUT",
            &format!("/v1/projects/demo/captures/{SECOND}"),
            Some(&ana),
            Some(&serde_json::to_string(&first).unwrap()),
        )
        .await;
        assert_eq!(
            status(&elsewhere),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{elsewhere}"
        );
        let mut blocked = first.clone();
        blocked.status = CaptureStatus::Blocked;
        let invalid = publish_record(port, &ana, &blocked).await;
        assert_eq!(
            status(&invalid),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{invalid}"
        );
        assert!(body(&invalid).contains("blocker"), "{invalid}");
        let mut snapshot = first.clone();
        snapshot.members[0].role = MemberRole::UiSnapshot;
        snapshot.members[0].path = "checkpoints/home.json".to_owned();
        let tree = publish_record(port, &ana, &snapshot).await;
        assert_eq!(status(&tree), StatusCode::UNPROCESSABLE_ENTITY, "{tree}");
        let reader = publish_record(port, &bruno, &first).await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
    }
}
