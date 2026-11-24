//! The panel's routes. Each module below this one is a path segment (`projects::project`
//! serves `/projects/{project}`); a handler reads the request and calls the page or action
//! that does the work. The layer below guards every route.

pub mod _workspace;
mod assets;
mod healthz;
mod login;
mod logout;
pub mod media;
mod register;

use crate::{
    Account, Arc, Body, Cx, IntoResponse, PageResult, Panel, Response, Router, StatusCode, auth,
    header, status,
};
use topcoat::{
    Result,
    context::context,
    router::{Next, OriginPolicy, RouterBuilderDiscoverExt, layer, module_router, request},
};

pub fn router(panel: Arc<Panel>) -> Router {
    let mut origins = vec![panel.origin.clone()];
    if !panel.secure {
        origins.push(panel.localhost_origin.clone());
    }
    module_router!()
        .discover()
        .with(OriginPolicy::trust_origins(origins))
        .with(panel)
        .build()
}

/// The panel's shared state.
pub(crate) fn panel(cx: &Cx) -> Arc<Panel> {
    context::<Arc<Panel>>(cx).clone()
}

/// The account the request is signed in as.
pub(crate) fn account(cx: &Cx) -> Account {
    context::<Account>(cx).clone()
}

pub(crate) fn respond(cx: &Cx, result: PageResult<impl IntoResponse>) -> Result<Response> {
    match result {
        Ok(response) => response.into_response(cx),
        Err(error) => Ok(status(error)),
    }
}

/// The error a failed read or action becomes: the router answers with its page for that status.
pub(crate) fn fail(status: StatusCode) -> topcoat::Error {
    use topcoat::router::error::{
        bad_request, forbidden, internal_server_error, not_found, service_unavailable,
        too_many_requests, unauthorized,
    };
    match status {
        StatusCode::UNAUTHORIZED => unauthorized().into(),
        StatusCode::FORBIDDEN => forbidden().into(),
        StatusCode::NOT_FOUND => not_found().into(),
        StatusCode::TOO_MANY_REQUESTS => too_many_requests(RETRY_AFTER_SECS).into(),
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE => {
            service_unavailable(RETRY_AFTER_SECS).into()
        }
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY | StatusCode::CONFLICT => {
            bad_request(status.to_string()).into()
        }
        other => internal_server_error(std::io::Error::other(other.to_string())).into(),
    }
}

/// What a rate-limited visitor is told to wait.
const RETRY_AFTER_SECS: u64 = 60;

#[layer]
async fn workspace(cx: &Cx, body: Body, next: Next<'_>) -> Result<Response> {
    let panel = panel(cx);
    let headers = request::headers(cx);
    let path = request::uri(cx).path();
    let received = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let local_alias = panel.localhost_origin.strip_prefix("http://");
    let local_loopback = panel
        .localhost_origin
        .strip_prefix("http://localhost:")
        .map(|p| format!("127.0.0.1:{p}"));
    let is_healthz = path == "/healthz" && received == local_loopback.as_deref();
    if !is_healthz && received != Some(&panel.host) && (panel.secure || received != local_alias) {
        return Ok(status(StatusCode::BAD_REQUEST));
    }
    let mut response = match auth::authenticate(&panel, path, headers, request::method(cx)).await {
        Ok(Some(account)) => next.run(&cx.with(account), body).await?,
        Ok(None) => next.run(cx, body).await?,
        Err(StatusCode::UNAUTHORIZED) => auth::unauthenticated(path),
        Err(code) => status(code),
    };
    let headers = response.headers_mut();
    if !headers.contains_key(header::CONTENT_SECURITY_POLICY) {
        headers.insert(header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; style-src 'self' 'unsafe-inline'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    }
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::REFERRER_POLICY, "same-origin".parse().unwrap());
    if !headers.contains_key(header::CACHE_CONTROL)
        && !path.starts_with("/media/a/")
        && !matches!(path, "/panel.css" | "/panel.js")
    {
        headers.insert(header::CACHE_CONTROL, "private, no-store".parse().unwrap());
    }
    if panel.secure {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            "max-age=31536000".parse().unwrap(),
        );
    }
    Ok(response)
}
