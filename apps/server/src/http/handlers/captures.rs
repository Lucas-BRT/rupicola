//! `/projects/{id}/captures`, `/captures/{id}`, `/artifacts/{id}` and the blob uploads.
//!
//! Uploading the old way is two steps: each file goes to `PUT /v1/blobs/{sha256}`
//! (skipped when `HEAD` says it is already stored), then `POST /v1/projects/{id}/captures`
//! describes the capture by referencing those digests.

use crate::{
    domain::{ApiError, Principal},
    services::{capture_runner, captures, uploads::Receiving},
    state::Shared,
};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Request, State},
    http::StatusCode,
    response::Response,
};
use rupicola_api::{
    Capture, CaptureSummary, Finding, Missing, MissingQuery, NewCapture, NewFinding, RunCapture,
};

/// 201 for a file stored now, 200 for one already kept.
const fn stored(created: bool) -> StatusCode {
    if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    }
}

/// `PUT /v1/blobs/{sha256}`
pub async fn put_blob(
    State(state): State<Shared>,
    _principal: Principal,
    Path(sha256): Path<String>,
    data: Bytes,
) -> Result<StatusCode, ApiError> {
    captures::put_blob(state, sha256, data).await.map(stored)
}

/// `HEAD /v1/blobs/{sha256}`: 200 when stored, so clients skip the upload.
pub async fn head_blob(
    State(state): State<Shared>,
    _principal: Principal,
    Path(sha256): Path<String>,
) -> Result<StatusCode, ApiError> {
    if captures::has_blob(&state, &sha256)? {
        Ok(StatusCode::OK)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `PUT /v1/projects/{id}/blobs/{sha256}`: the file arrives as a stream, cut when it takes
/// longer than the project allows in all or between two bytes.
pub async fn upload(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, sha256)): Path<(String, String)>,
    mut body: Body,
) -> Result<StatusCode, ApiError> {
    use http_body_util::BodyExt as _;
    let mut receiving = Receiving::begin(&state, &principal, &project, &sha256).await?;
    let deadline = tokio::time::Instant::now() + receiving.timeout();
    let idle = receiving.idle();
    loop {
        let wait = idle.min(deadline.saturating_duration_since(tokio::time::Instant::now()));
        let frame = match tokio::time::timeout(wait, body.frame()).await {
            Err(_) => return Err(ApiError::Timeout),
            Ok(None) => break,
            Ok(Some(frame)) => {
                frame.map_err(|e| ApiError::invalid(format!("the upload broke off: {e}")))?
            }
        };
        if let Ok(chunk) = frame.into_data() {
            receiving.take(&chunk).await?;
        }
    }
    receiving.finish(&state).await.map(stored)
}

/// `POST /v1/projects/{id}/blobs/missing`
pub async fn missing(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(query): Json<MissingQuery>,
) -> Result<Json<Missing>, ApiError> {
    captures::missing(&state, &principal, &project, &query)
        .await
        .map(Json)
}

/// `POST /v1/projects/{id}/captures`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewCapture>,
) -> Result<(StatusCode, Json<Capture>), ApiError> {
    let capture = captures::create(&state, &principal, &project, form).await?;
    Ok((StatusCode::CREATED, Json(capture)))
}

/// `POST /v1/projects/{id}/captures/run`: captures `form.url` using the server's external
/// capture runner and stores the resulting capture (`complete` or `blocked`).
pub async fn run(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<RunCapture>,
) -> Result<(StatusCode, Json<Capture>), ApiError> {
    let capture = capture_runner::run(&state, &principal, &project, form).await?;
    Ok((StatusCode::CREATED, Json(capture)))
}

/// `GET /v1/projects/{id}/captures`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<CaptureSummary>>, ApiError> {
    captures::list(&state, &principal, &project).await.map(Json)
}

/// `GET /v1/captures/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Capture>, ApiError> {
    captures::get(&state, &principal, &id).await.map(Json)
}

/// `POST /v1/captures/{id}/findings`
pub async fn add_finding(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<NewFinding>,
) -> Result<(StatusCode, Json<Finding>), ApiError> {
    let finding = captures::add_finding(&state, &principal, &id, form).await?;
    Ok((StatusCode::CREATED, Json(finding)))
}

/// `GET /v1/artifacts/{id}`: the file's bytes, for readers of its project.
pub async fn artifact(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    let blob = captures::artifact(&state, &principal, &id).await?;
    crate::http::blob::respond(&state.blobs, &blob.sha256, &blob.mime, request).await
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Capture, CaptureSummary, Finding, Inspection};
    use sha2::{Digest, Sha256};

    fn digest(data: &[u8]) -> String {
        format!("{:x}", Sha256::digest(data))
    }

    async fn upload(port: u16, token: &str, data: &[u8]) -> String {
        let sha = digest(data);
        let response = request(
            port,
            format!(
                "PUT /v1/blobs/{sha} HTTP/1.1\r\nHost: test\r\nAuthorization: Bearer {token}\r\n\
                 Content-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                data.len(),
                String::from_utf8_lossy(data)
            ),
        )
        .await;
        assert!(
            matches!(status(&response), StatusCode::OK | StatusCode::CREATED),
            "{response}"
        );
        sha
    }

    /// Ana owns `demo`; Bruno reads it; Carla is an outsider. Returns their tokens.
    async fn demo() -> (u16, [String; USERS]) {
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
        (port, [ana, bruno, carla])
    }

    /// Ana (owner), Bruno (reader) and Carla (outsider).
    const USERS: usize = 3;
    const VIEWPORT_WIDTH: u32 = 1280;
    const VIEWPORT_HEIGHT: u32 = 720;
    const FRAME_TARGET_MS: f64 = 100.0;

    fn capture_json(home: &str, frame: &str) -> String {
        format!(
            r#"{{"scenario":"home","status":"complete",
                "environment":{{"width":{VIEWPORT_WIDTH},"height":{VIEWPORT_HEIGHT},"color_scheme":"dark"}},
                "checkpoints":[{{"id":"home","path":"checkpoints/home.png","sha256":"{home}"}}],
                "clips":[{{"id":"open","frames":[{{"path":"clips/open/1.png","sha256":"{frame}","target_ms":{FRAME_TARGET_MS}}}]}}]}}"#
        )
    }

    #[tokio::test]
    async fn blobs_are_verified_and_deduplicated() {
        let (port, [ana, ..]) = demo().await;
        let sha = upload(port, &ana, b"png-bytes").await;
        let again = request(
            port,
            format!("PUT /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {ana}\r\nContent-Length: 9\r\nConnection: close\r\n\r\npng-bytes"),
        )
        .await;
        assert_eq!(status(&again), StatusCode::OK, "{again}");
        let head = request(port, format!("HEAD /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {ana}\r\nConnection: close\r\n\r\n")).await;
        assert_eq!(status(&head), StatusCode::OK, "{head}");
        let forged = request(
            port,
            format!("PUT /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {ana}\r\nContent-Length: 5\r\nConnection: close\r\n\r\nother"),
        )
        .await;
        assert_eq!(
            status(&forged),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{forged}"
        );
        let anonymous = request(
            port,
            format!("HEAD /v1/blobs/{sha} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n"),
        )
        .await;
        assert_eq!(status(&anonymous), StatusCode::UNAUTHORIZED, "{anonymous}");
    }

    #[tokio::test]
    async fn a_published_capture_can_be_browsed_reviewed_and_downloaded() {
        let (port, [ana, bruno, _]) = demo().await;
        let home = upload(port, &ana, b"home-png").await;
        let frame = upload(port, &ana, b"frame-png").await;
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&capture_json(&home, &frame)),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let capture: Capture = json(&created);
        assert_eq!(capture.checkpoints[0].artifact.mime, "image/png");
        assert_eq!(capture.clips[0].frames[0].target_ms, FRAME_TARGET_MS);
        assert_eq!(capture.environment.width, Some(VIEWPORT_WIDTH));
        assert_eq!(capture.inspection, Inspection::Pending);

        let listed: Vec<CaptureSummary> = json(
            &call(
                port,
                "GET",
                "/v1/projects/demo/captures",
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].preview.as_deref(),
            Some(capture.checkpoints[0].artifact.id.as_str())
        );

        let artifact = &capture.checkpoints[0].artifact.id;
        let file = call(
            port,
            "GET",
            &format!("/v1/artifacts/{artifact}"),
            Some(&bruno),
            None,
        )
        .await;
        assert_eq!(status(&file), StatusCode::OK, "{file}");
        assert_eq!(header(&file, "content-type"), Some("image/png"));
        assert_eq!(body(&file), "home-png");

        let note = format!(r#"{{"note":"Title overflows","evidence_artifact_id":"{artifact}"}}"#);
        let readers_cannot = call(
            port,
            "POST",
            &format!("/v1/captures/{}/findings", capture.id),
            Some(&bruno),
            Some(&note),
        )
        .await;
        assert_eq!(
            status(&readers_cannot),
            StatusCode::FORBIDDEN,
            "{readers_cannot}"
        );
        let added = call(
            port,
            "POST",
            &format!("/v1/captures/{}/findings", capture.id),
            Some(&ana),
            Some(&note),
        )
        .await;
        assert_eq!(status(&added), StatusCode::CREATED, "{added}");
        let finding: Finding = json(&added);
        assert_eq!(finding.author.as_deref(), Some("ana"));

        let reviewed: Capture = json(
            &call(
                port,
                "GET",
                &format!("/v1/captures/{}", capture.id),
                Some(&bruno),
                None,
            )
            .await,
        );
        assert_eq!(reviewed.inspection, Inspection::Reviewed);
        assert_eq!(reviewed.findings, [finding]);
    }

    #[tokio::test]
    async fn publishing_needs_uploaded_files_and_a_reviewer() {
        let (port, [ana, bruno, carla]) = demo().await;
        let home = upload(port, &ana, b"home-png").await;
        let missing = digest(b"never uploaded");
        let incomplete = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&capture_json(&home, &missing)),
        )
        .await;
        assert_eq!(
            status(&incomplete),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{incomplete}"
        );

        let frame = upload(port, &ana, b"frame-png").await;
        let body = capture_json(&home, &frame);
        let reader = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&bruno),
            Some(&body),
        )
        .await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN, "{reader}");
        let outsider = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&carla),
            Some(&body),
        )
        .await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND, "{outsider}");

        let escaping = body.replace("checkpoints/home.png", "../home.png");
        let bad_path = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&escaping),
        )
        .await;
        assert_eq!(
            status(&bad_path),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad_path}"
        );
    }

    #[tokio::test]
    async fn outsiders_cannot_read_captures_or_files() {
        let (port, [ana, _, carla]) = demo().await;
        let home = upload(port, &ana, b"home-png").await;
        let frame = upload(port, &ana, b"frame-png").await;
        let capture: Capture = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/captures",
                Some(&ana),
                Some(&capture_json(&home, &frame)),
            )
            .await,
        );
        for path in [
            format!("/v1/captures/{}", capture.id),
            format!("/v1/artifacts/{}", capture.checkpoints[0].artifact.id),
            "/v1/projects/demo/captures".to_owned(),
        ] {
            let response = call(port, "GET", &path, Some(&carla), None).await;
            assert_eq!(
                status(&response),
                StatusCode::NOT_FOUND,
                "{path}: {response}"
            );
        }
    }

    fn missing_body(hashes: &[&str]) -> String {
        serde_json::to_string(&serde_json::json!({ "sha256": hashes })).unwrap()
    }

    #[tokio::test]
    async fn only_the_files_a_project_does_not_hold_are_missing_and_once_each() {
        let (port, [ana, _, carla]) = demo().await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&carla),
            Some(r#"{"id":"other","label":"Other"}"#),
        )
        .await;
        let home = upload(port, &ana, b"home pixels").await;
        let frame = upload(port, &ana, b"frame pixels").await;
        let fresh = digest(b"never uploaded");
        let ask = |token: &str, project: &str, hashes: &[&str]| {
            let (token, path, body) = (
                token.to_owned(),
                format!("/v1/projects/{project}/blobs/missing"),
                missing_body(hashes),
            );
            async move { call(port, "POST", &path, Some(&token), Some(&body)).await }
        };
        // Stored, but no capture of the project cites them yet.
        let before = ask(&ana, "demo", &[&home, &fresh, &home]).await;
        assert_eq!(status(&before), StatusCode::OK, "{before}");
        let before: rupicola_api::Missing = json(&before);
        assert_eq!(
            before.missing,
            [home.clone(), fresh.clone()],
            "in order and once each"
        );
        assert!(before.limits.max_blob_bytes > 0, "the limits come along");
        let created = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&ana),
            Some(&capture_json(&home, &frame)),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let after: rupicola_api::Missing = json(&ask(&ana, "demo", &[&home, &frame, &fresh]).await);
        assert_eq!(after.missing, std::slice::from_ref(&fresh));
        let elsewhere: rupicola_api::Missing = json(&ask(&carla, "other", &[&home]).await);
        assert_eq!(
            elsewhere.missing,
            [home],
            "another project does not learn of the blob"
        );
    }

    #[tokio::test]
    async fn asking_what_is_missing_needs_upload_rights_and_plain_hashes() {
        let (port, [ana, bruno, carla]) = demo().await;
        let path = "/v1/projects/demo/blobs/missing";
        let good = missing_body(&[&digest(b"x")]);
        let reader = call(port, "POST", path, Some(&bruno), Some(&good)).await;
        assert_eq!(
            status(&reader),
            StatusCode::FORBIDDEN,
            "a reader does not upload"
        );
        let outsider = call(port, "POST", path, Some(&carla), Some(&good)).await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND);
        let shouting = missing_body(&[&digest(b"x").to_uppercase()]);
        let bad = call(port, "POST", path, Some(&ana), Some(&shouting)).await;
        assert_eq!(status(&bad), StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        let many: Vec<String> = (0..=rupicola_api::MAX_MISSING_BATCH)
            .map(|i| digest(i.to_string().as_bytes()))
            .collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        let too_many = call(port, "POST", path, Some(&ana), Some(&missing_body(&refs))).await;
        assert_eq!(
            status(&too_many),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{too_many}"
        );
    }

    /// A PNG's first bytes, so the upload is told to be an image.
    const PNG_HEAD: &[u8] = &[
        0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 13,
    ];

    /// Sends `data` to the streamed upload; `declared` is the length the request claims.
    async fn stream(port: u16, token: &str, sha: &str, data: &[u8], declared: usize) -> String {
        request_bytes(
            port,
            format!(
                "PUT /v1/projects/demo/blobs/{sha} HTTP/1.1\r\nHost: test\r\n\
                 Authorization: Bearer {token}\r\nContent-Type: application/octet-stream\r\n\
                 Content-Length: {declared}\r\nConnection: close\r\n\r\n"
            ),
            data.to_vec(),
        )
        .await
    }

    async fn missing_of(port: u16, token: &str, sha: &str) -> Vec<String> {
        let response = call(
            port,
            "POST",
            "/v1/projects/demo/blobs/missing",
            Some(token),
            Some(&missing_body(&[sha])),
        )
        .await;
        json::<rupicola_api::Missing>(&response).missing
    }

    #[tokio::test]
    async fn a_streamed_upload_is_stored_once_and_the_project_then_holds_it() {
        let (port, [ana, ..]) = demo().await;
        let data = b"streamed pixels";
        let sha = digest(data);
        assert_eq!(
            missing_of(port, &ana, &sha).await,
            std::slice::from_ref(&sha)
        );
        let first = stream(port, &ana, &sha, data, data.len()).await;
        assert_eq!(status(&first), StatusCode::CREATED, "{first}");
        let again = stream(port, &ana, &sha, data, data.len()).await;
        assert_eq!(status(&again), StatusCode::OK, "{again}");
        assert_eq!(missing_of(port, &ana, &sha).await, Vec::<String>::new());
    }

    #[tokio::test]
    async fn content_that_does_not_match_its_digest_is_not_kept() {
        let (port, [ana, ..]) = demo().await;
        let sha = digest(b"what was promised");
        let lie = b"something else";
        let response = stream(port, &ana, &sha, lie, lie.len()).await;
        assert_eq!(
            status(&response),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{response}"
        );
        assert_eq!(missing_of(port, &ana, &sha).await, [sha]);
    }

    #[tokio::test]
    async fn the_limit_follows_what_the_file_is_by_its_first_bytes() {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        let ana = sign_in(port, "ana").await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        sqlx::query(
            "INSERT INTO project_settings (project_id, key, value, updated_at)
             VALUES ('demo', 'max_image_bytes', '16', 0)",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let mut image = PNG_HEAD.to_vec();
        image.extend_from_slice(b"more than sixteen bytes in all");
        let response = stream(port, &ana, &digest(&image), &image, image.len()).await;
        assert_eq!(
            status(&response),
            StatusCode::PAYLOAD_TOO_LARGE,
            "{response}"
        );
        let text = b"the same length but not an image";
        let response = stream(port, &ana, &digest(text), text, text.len()).await;
        assert_eq!(status(&response), StatusCode::CREATED, "{response}");
    }

    #[tokio::test]
    async fn nothing_larger_than_the_ceiling_gets_in() {
        let (port, [ana, ..]) = demo().await;
        let data = vec![b'x'; settings().blob_limit + 1];
        let response = stream(port, &ana, &digest(&data), &data, data.len()).await;
        assert_eq!(
            status(&response),
            StatusCode::PAYLOAD_TOO_LARGE,
            "{response}"
        );
    }

    #[tokio::test]
    async fn an_upload_that_stops_sending_is_cut_after_the_idle_limit() {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        let ana = sign_in(port, "ana").await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        sqlx::query(
            "INSERT INTO project_settings (project_id, key, value, updated_at)
             VALUES ('demo', 'upload_idle_secs', '1', 0)",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let started = std::time::Instant::now();
        // The request promises more bytes than it sends, then goes quiet.
        let response = stream(port, &ana, &digest(b"never finished"), b"never", 100).await;
        assert_eq!(status(&response), StatusCode::REQUEST_TIMEOUT, "{response}");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[tokio::test]
    async fn only_people_who_may_upload_stream_a_file() {
        let (port, [_, bruno, carla]) = demo().await;
        let data = b"pixels";
        let reader = stream(port, &bruno, &digest(data), data, data.len()).await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN, "{reader}");
        let outsider = stream(port, &carla, &digest(data), data, data.len()).await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND, "{outsider}");
    }

    #[tokio::test]
    async fn running_a_capture_validates_target_and_enforces_access() {
        let (port, [ana, bruno, carla]) = demo().await;
        let denied = call(
            port,
            "POST",
            "/v1/projects/demo/captures/run",
            Some(&bruno),
            Some(r#"{"scenario":"Home","url":"http://127.0.0.1:4731/"}"#),
        )
        .await;
        assert_eq!(status(&denied), StatusCode::FORBIDDEN, "{denied}");

        let outsider = call(
            port,
            "POST",
            "/v1/projects/demo/captures/run",
            Some(&carla),
            Some(r#"{"scenario":"Home","url":"http://127.0.0.1:4731/"}"#),
        )
        .await;
        assert_eq!(status(&outsider), StatusCode::NOT_FOUND, "{outsider}");

        let invalid = call(
            port,
            "POST",
            "/v1/projects/demo/captures/run",
            Some(&ana),
            Some(r#"{"scenario":"Bad","url":"file:///etc/passwd"}"#),
        )
        .await;
        assert_eq!(
            status(&invalid),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{invalid}"
        );

        let ran = call(
            port,
            "POST",
            "/v1/projects/demo/captures/run",
            Some(&ana),
            Some(r#"{"scenario":"Home","url":"http://127.0.0.1:9/"}"#),
        )
        .await;
        assert_eq!(status(&ran), StatusCode::CREATED, "{ran}");
        let capture: Capture = json(&ran);
        assert_eq!(capture.scenario, "Home");
    }
}
