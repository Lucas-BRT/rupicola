use super::{
    Account, Arc, Body, Capture, PageResult, Panel, Request, Response, StatusCode, auth, header,
};
use rupicola_client::ArtifactBody;

/// Where the panel serves the artifact with this id.
pub fn url(artifact_id: &str) -> String {
    format!("/media/a/{artifact_id}")
}

/// Serves an artifact by id: `/media/a/{id}`. The backend checks that the account may read
/// the project it belongs to; the panel passes on the byte range a browser asks for, so a
/// recording can be seeked.
pub async fn by_id(
    account: Account,
    panel: Arc<Panel>,
    id: String,
    request: Request,
) -> PageResult<Response> {
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let artifact = panel
        .api
        .artifact(&account.session, &id, range)
        .await
        .map_err(|e| auth::api_error(&e))?;
    response_of(artifact)
}

fn response_of(artifact: ArtifactBody) -> PageResult<Response> {
    let mut response = Response::new(Body::from(artifact.bytes));
    *response.status_mut() =
        StatusCode::from_u16(artifact.status).map_err(|_| StatusCode::BAD_GATEWAY)?;
    let headers = response.headers_mut();
    for (name, value) in [
        (header::CONTENT_TYPE, artifact.content_type),
        (header::CONTENT_RANGE, artifact.content_range),
    ] {
        if let Some(value) = value.and_then(|value| header::HeaderValue::from_str(&value).ok()) {
            headers.insert(name, value);
        }
    }
    headers.insert(
        header::ACCEPT_RANGES,
        header::HeaderValue::from_static("bytes"),
    );
    Ok(response)
}

pub fn declared(capture: &Capture, artifact_id: &str) -> bool {
    capture
        .checkpoints
        .iter()
        .any(|item| item.artifact.id == artifact_id)
        || capture.clips.iter().any(|clip| {
            [
                clip.video.as_ref().map(|a| a.id.as_str()),
                clip.sheet.as_ref().map(|a| a.id.as_str()),
                clip.analysis.as_ref().map(|a| a.id.as_str()),
            ]
            .contains(&Some(artifact_id))
                || clip
                    .frames
                    .iter()
                    .any(|frame| frame.artifact.id == artifact_id)
        })
}
