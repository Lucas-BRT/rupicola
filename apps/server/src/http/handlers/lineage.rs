//! `/captures/{id}/parents`, `/captures/{id}/lineage` and `/captures/{a}/compare/{b}`.

use crate::{
    domain::{ApiError, Principal},
    services::lineage,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use rupicola_api::{CaptureDiff, Lineage, LineageGraph, ParentsEdit};

/// How far a lineage graph should reach.
#[derive(serde::Deserialize)]
pub struct Reach {
    depth: Option<u32>,
}

/// `GET /v1/captures/{id}/lineage?depth=N`
pub async fn graph(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
    Query(reach): Query<Reach>,
) -> Result<Json<LineageGraph>, ApiError> {
    lineage::graph(&state, &principal, &capture, reach.depth)
        .await
        .map(Json)
}

/// `GET /v1/captures/{a}/compare/{b}`
pub async fn compare(
    State(state): State<Shared>,
    principal: Principal,
    Path((before, after)): Path<(String, String)>,
) -> Result<Json<CaptureDiff>, ApiError> {
    lineage::compare(&state, &principal, &before, &after)
        .await
        .map(Json)
}

/// `GET /v1/captures/{id}/parents`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
) -> Result<Json<Lineage>, ApiError> {
    lineage::get(&state, &principal, &capture).await.map(Json)
}

/// `PUT /v1/captures/{id}/parents`
pub async fn edit(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
    Json(form): Json<ParentsEdit>,
) -> Result<Json<Lineage>, ApiError> {
    lineage::edit(&state, &principal, &capture, &form)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{
        CaptureDiff, Lineage, LineageGraph, ParentOrigin,
        record::{ParentKind, ParentRef},
    };

    const A: &str = "01936f2c-aaaa-7bbb-8ccc-00000000000a";
    const B: &str = "01936f2c-aaaa-7bbb-8ccc-00000000000b";
    const C: &str = "01936f2c-aaaa-7bbb-8ccc-00000000000c";
    const D: &str = "01936f2c-aaaa-7bbb-8ccc-00000000000d";

    fn parent(id: &str, kind: ParentKind) -> ParentRef {
        ParentRef {
            id: id.to_owned(),
            kind,
        }
    }

    /// Ana owns `demo` with captures A, B (child of A) and C; Bruno reads; Carla is outside.
    async fn demo() -> (u16, [String; 3]) {
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
        for (id, parents) in [
            (A, Vec::new()),
            (B, vec![parent(A, ParentKind::Previous)]),
            (C, Vec::new()),
        ] {
            let published = publish_record(port, &ana, &capture_record(id, &sha, 6, parents)).await;
            assert_eq!(status(&published), StatusCode::CREATED, "{published}");
        }
        (port, [ana, bruno, carla])
    }

    async fn edit(
        port: u16,
        token: &str,
        capture: &str,
        parents: &[ParentRef],
        generation: i64,
        reason: &str,
    ) -> String {
        let body =
            serde_json::json!({ "parents": parents, "generation": generation, "reason": reason });
        call(
            port,
            "PUT",
            &format!("/v1/captures/{capture}/parents"),
            Some(token),
            Some(&body.to_string()),
        )
        .await
    }

    async fn lineage(port: u16, token: &str, capture: &str) -> String {
        call(
            port,
            "GET",
            &format!("/v1/captures/{capture}/parents"),
            Some(token),
            None,
        )
        .await
    }

    #[tokio::test]
    async fn the_record_parents_can_be_replaced_and_the_change_is_remembered() {
        let (port, [ana, bruno, _]) = demo().await;
        let read: Lineage = json(&lineage(port, &bruno, B).await);
        assert_eq!(read.parents.len(), 1);
        assert_eq!(
            (read.parents[0].id.as_str(), read.parents[0].origin),
            (A, ParentOrigin::Record)
        );
        assert_eq!((read.generation, read.history.len()), (0, 0));
        let edited = edit(
            port,
            &ana,
            B,
            &[parent(C, ParentKind::Baseline)],
            0,
            "compared against C",
        )
        .await;
        assert_eq!(status(&edited), StatusCode::OK, "{edited}");
        let after: Lineage = json(&edited);
        assert_eq!(after.parents[0].id, C);
        assert_eq!(after.parents[0].kind, ParentKind::Baseline);
        assert_eq!(after.parents[0].origin, ParentOrigin::Edit);
        assert_eq!(after.generation, 1);
        let event = &after.history[0];
        assert_eq!(
            (event.actor.as_str(), event.reason.as_str()),
            ("ana", "compared against C")
        );
        assert_eq!(event.before, [parent(A, ParentKind::Previous)]);
        assert_eq!(event.after, [parent(C, ParentKind::Baseline)]);
        let stale = edit(port, &ana, B, &[], 0, "again from the old read").await;
        assert_eq!(status(&stale), StatusCode::CONFLICT, "{stale}");
        let cleared = edit(port, &ana, B, &[], 1, "no parent after all").await;
        assert_eq!(json::<Lineage>(&cleared).parents.len(), 0);
    }

    #[tokio::test]
    async fn an_edit_cannot_make_a_loop_or_point_outside_the_project() {
        let (port, [ana, ..]) = demo().await;
        let looping = edit(port, &ana, A, &[parent(B, ParentKind::Previous)], 0, "loop").await;
        assert_eq!(
            status(&looping),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{looping}"
        );
        assert!(body(&looping).contains("descends"), "{looping}");
        let unchanged: Lineage = json(&lineage(port, &ana, A).await);
        assert_eq!(unchanged.generation, 0, "a refused edit changes nothing");
        for (parents, why) in [
            (vec![parent(A, ParentKind::Previous)], "own parent"),
            (
                vec![parent(C, ParentKind::Input), parent(C, ParentKind::Input)],
                "twice",
            ),
            (vec![parent("nope", ParentKind::Input)], "nope"),
        ] {
            let response = edit(port, &ana, A, &parents, 0, "test").await;
            assert_eq!(
                status(&response),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{response}"
            );
            assert!(body(&response).contains(why), "{why}: {response}");
        }
        let silent = edit(port, &ana, A, &[], 0, "  ").await;
        assert_eq!(status(&silent), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn readers_see_the_lineage_and_only_reviewers_change_it() {
        let (port, [_, bruno, carla]) = demo().await;
        let reader = edit(port, &bruno, B, &[], 0, "not mine to change").await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
        let outsider = lineage(port, &carla, B).await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND);
        let missing = lineage(port, &bruno, "no-such-capture").await;
        assert_eq!(status(&missing), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_graph_reaches_up_and_down_as_far_as_asked() {
        let (port, [ana, bruno, _]) = demo().await;
        edit(
            port,
            &ana,
            C,
            &[parent(B, ParentKind::Previous)],
            0,
            "C continues B",
        )
        .await;
        let graph = |capture: &str, depth: &str| {
            let (path, token) = (
                format!("/v1/captures/{capture}/lineage{depth}"),
                bruno.clone(),
            );
            async move { call(port, "GET", &path, Some(&token), None).await }
        };
        let whole: LineageGraph = json(&graph(B, "").await);
        let mut nodes: Vec<&str> = whole.nodes.iter().map(|n| n.id.as_str()).collect();
        nodes.sort_unstable();
        assert_eq!(nodes, [A, B, C], "one parent up and one child down");
        assert_eq!(whole.links.len(), 2);
        let near: LineageGraph = json(&graph(A, "?depth=1").await);
        let mut nodes: Vec<&str> = near.nodes.iter().map(|n| n.id.as_str()).collect();
        nodes.sort_unstable();
        assert_eq!(nodes, [A, B], "C is two steps away");
        assert_eq!(near.links[0].origin, ParentOrigin::Record);
        for bad in ["?depth=0", "?depth=21"] {
            assert_eq!(
                status(&graph(A, bad).await),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn a_comparison_pairs_files_by_path_and_lists_the_facets_that_differ() {
        let (port, [ana, bruno, carla]) = demo().await;
        let same: CaptureDiff = json(
            &call(
                port,
                "GET",
                &format!("/v1/captures/{A}/compare/{B}"),
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(same.same, ["checkpoints/home.png"]);
        assert!(same.changed.is_empty() && same.added.is_empty() && same.removed.is_empty());
        assert_eq!(same.facets, Vec::new());
        let menu = stream_blob(port, &ana, "demo", b"other pixels").await;
        let mut other = capture_record(D, &menu, 12, Vec::new());
        other.members[0].path = "checkpoints/menu.png".to_owned();
        other.environment.appearance = Some(rupicola_api::record::Appearance {
            color_scheme: Some("light".to_owned()),
            ..rupicola_api::record::Appearance::default()
        });
        publish_record(port, &ana, &other).await;
        let diff: CaptureDiff = json(
            &call(
                port,
                "GET",
                &format!("/v1/captures/{A}/compare/{D}"),
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(diff.removed, ["checkpoints/home.png"]);
        assert_eq!(diff.added, ["checkpoints/menu.png"]);
        assert_eq!(diff.facets.len(), 1);
        assert_eq!(diff.facets[0].key, "appearance.color_scheme");
        assert_eq!(
            (
                diff.facets[0].before.as_deref(),
                diff.facets[0].after.as_deref()
            ),
            (Some("dark"), Some("light"))
        );
        let outsider = call(
            port,
            "GET",
            &format!("/v1/captures/{A}/compare/{B}"),
            Some(&carla),
            None,
        )
        .await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND);
        let unknown = call(
            port,
            "GET",
            &format!("/v1/captures/{A}/compare/nope"),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&unknown), StatusCode::NOT_FOUND);
    }
}
