//! `GET /projects/{project}/jobs`: JSON list of open capture jobs for live panel polling.

use super::current_meta;
use crate::{app::fail, backend};
use http::header;
use topcoat::{Result, context::Cx, router::route};

#[route(GET)]
async fn list(cx: &Cx) -> Result<([(http::HeaderName, &'static str); 1], String)> {
    let project = current_meta(cx).await.as_ref().map_err(|e| fail(*e))?;
    let jobs = backend::open_jobs(cx, &project.id)
        .await
        .as_ref()
        .map_err(|e| fail(*e))?;
    let body =
        serde_json::to_string(&jobs).map_err(|_| fail(http::StatusCode::INTERNAL_SERVER_ERROR))?;
    Ok(([(header::CONTENT_TYPE, "application/json")], body))
}
