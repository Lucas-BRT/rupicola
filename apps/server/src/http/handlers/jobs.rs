//! `/projects/{id}/jobs` and `/jobs/{id}` handlers.

use crate::{
    domain::{ApiError, Principal},
    services::jobs,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rupicola_api::{Job, JobFailure, JobProgress, JobResult, NewJob};
use serde::Deserialize;

/// `GET /v1/projects/{id}/jobs?open=true`
#[derive(Deserialize)]
pub struct ListQuery {
    /// Only the jobs still waiting or running.
    #[serde(default)]
    open: bool,
}

/// `POST /v1/projects/{id}/jobs`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewJob>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    jobs::create(&state, &principal, &project, &form)
        .await
        .map(|job| (StatusCode::CREATED, Json(job)))
}

/// `GET /v1/projects/{id}/jobs`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Job>>, ApiError> {
    jobs::list(&state, &principal, &project, query.open)
        .await
        .map(Json)
}

/// `POST /v1/projects/{id}/jobs/claim`: 200 with the job the caller now holds, or 204 when
/// there is nothing to do.
pub async fn claim(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Response, ApiError> {
    Ok(match jobs::claim(&state, &principal, &project).await? {
        Some(job) => Json(job).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}

/// `GET /v1/jobs/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Job>, ApiError> {
    jobs::get(&state, &principal, &id).await.map(Json)
}

/// `POST /v1/jobs/{id}/progress`
pub async fn progress(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(progress): Json<JobProgress>,
) -> Result<Json<Job>, ApiError> {
    jobs::progress(&state, &principal, &id, progress)
        .await
        .map(Json)
}

/// `POST /v1/jobs/{id}/complete`
pub async fn complete(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(result): Json<JobResult>,
) -> Result<Json<Job>, ApiError> {
    jobs::complete(&state, &principal, &id, &result)
        .await
        .map(Json)
}

/// `POST /v1/jobs/{id}/fail`
pub async fn fail(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(failure): Json<JobFailure>,
) -> Result<Json<Job>, ApiError> {
    jobs::fail(&state, &principal, &id, &failure)
        .await
        .map(Json)
}

/// `POST /v1/jobs/{id}/cancel`
pub async fn cancel(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Job>, ApiError> {
    jobs::cancel(&state, &principal, &id).await.map(Json)
}

#[cfg(test)]
mod tests {
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
}
