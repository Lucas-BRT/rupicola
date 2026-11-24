//! `/projects/{id}/scenarios` handlers.

use crate::{
    domain::{ApiError, Principal},
    services::scenarios,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{Scenario, ScenarioSummary, ScenarioVersion};

/// `POST /v1/projects/{id}/scenarios`: 201 for a definition the project has not had, 200 when
/// the same one is pushed again.
pub async fn put(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(scenario): Json<Scenario>,
) -> Result<(StatusCode, Json<ScenarioVersion>), ApiError> {
    let (version, created) = scenarios::put(&state, &principal, &project, &scenario).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(version)))
}

/// `GET /v1/projects/{id}/scenarios`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<ScenarioSummary>>, ApiError> {
    scenarios::list(&state, &principal, &project)
        .await
        .map(Json)
}

/// `GET /v1/projects/{id}/scenarios/{name}`
pub async fn latest(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, name)): Path<(String, String)>,
) -> Result<Json<ScenarioVersion>, ApiError> {
    scenarios::latest(&state, &principal, &project, &name)
        .await
        .map(Json)
}

/// `GET /v1/projects/{id}/scenarios/{name}/{hash}`
pub async fn version(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, name, hash)): Path<(String, String, String)>,
) -> Result<Json<ScenarioVersion>, ApiError> {
    scenarios::version(&state, &principal, &project, &name, &hash)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Scenario, ScenarioSummary, ScenarioVersion};

    const HOME: &str = r#"{"name":"home","url":"/",
        "matrix":{"viewports":["390x844@2","1280x800"],"themes":["dark","light"]},
        "steps":[{"kind":"checkpoint","id":"full","full_page":true}]}"#;
    const HOME_DOCS: &str = r#"{"name":"home","url":"/docs"}"#;

    async fn push(port: u16, token: &str, body: &str) -> String {
        call(
            port,
            "POST",
            "/v1/projects/demo/scenarios",
            Some(token),
            Some(body),
        )
        .await
    }

    #[tokio::test]
    async fn a_pushed_definition_is_kept_by_its_hash_and_pushing_it_again_is_a_no_op() {
        let (port, [ana, bruno, _]) = demo_with_artifacts().await;
        let first = push(port, &ana, HOME).await;
        assert_eq!(status(&first), StatusCode::CREATED, "{first}");
        let version: ScenarioVersion = json(&first);
        assert_eq!(version.hash.len(), 64);
        assert_eq!(version.scenario.matrix.cells(), 4);

        let again = push(port, &ana, HOME).await;
        assert_eq!(status(&again), StatusCode::OK, "{again}");
        assert_eq!(json::<ScenarioVersion>(&again).hash, version.hash);

        let listed: Vec<ScenarioSummary> = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/scenarios",
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].name.as_str(), listed[0].cells), ("home", 4));
    }

    #[tokio::test]
    async fn the_latest_version_is_the_last_pushed_and_older_ones_stay_readable() {
        let (port, [ana, _, _]) = demo_with_artifacts().await;
        let old: ScenarioVersion = json(&push(port, &ana, HOME).await);
        // Pushed in a later millisecond so the order is unambiguous.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let new: ScenarioVersion = json(&push(port, &ana, HOME_DOCS).await);
        assert_ne!(old.hash, new.hash);

        let latest: ScenarioVersion = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/scenarios/home",
                Some(&ana),
                None,
            )
            .await,
        );
        assert_eq!(latest.hash, new.hash);
        let path = format!("/v1/projects/demo/scenarios/home/{}", old.hash);
        let fetched: ScenarioVersion = json(&call(port, "GET", &path, Some(&ana), None).await);
        let expected: Scenario = serde_json::from_str(HOME).unwrap();
        assert_eq!(fetched.scenario, expected);

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        push(port, &ana, HOME).await;
        let latest: ScenarioVersion = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/scenarios/home",
                Some(&ana),
                None,
            )
            .await,
        );
        assert_eq!(
            latest.hash, old.hash,
            "pushing an old version makes it the latest again"
        );
    }

    #[tokio::test]
    async fn an_invalid_definition_is_refused_and_a_reader_cannot_push() {
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
        let bad = push(port, &ana, r#"{"name":"home","url":"http://evil.test/"}"#).await;
        assert_eq!(status(&bad), StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        let typo = push(port, &ana, r#"{"name":"home","url":"/","chekcs":{}}"#).await;
        assert!(status(&typo).is_client_error(), "{typo}");
        for token in [&bruno, &carla] {
            let denied = push(port, token, HOME).await;
            assert!(status(&denied).is_client_error(), "{denied}");
        }
        let missing = call(
            port,
            "GET",
            "/v1/projects/demo/scenarios/none",
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&missing), StatusCode::NOT_FOUND, "{missing}");
    }
}
