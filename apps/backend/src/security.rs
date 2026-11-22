//! Request hardening applied to every route: per-client rate limiting, body size,
//! timeout and concurrency limits, panic isolation, security headers, request ids
//! and redaction of credentials from logs.

use crate::error::ApiError;
use axum::{
    Router,
    error_handling::HandleErrorLayer,
    extract::ConnectInfo,
    http::{HeaderName, HeaderValue, Request, header},
    response::IntoResponse,
};
use governor::{clock::QuantaInstant, middleware::NoOpMiddleware};
use std::{net::IpAddr, net::SocketAddr, sync::Arc, time::Duration};
use tower::ServiceBuilder;
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder, key_extractor::KeyExtractor,
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    sensitive_headers::SetSensitiveRequestHeadersLayer,
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

const MIB: usize = 1024 * 1024;
/// Default requests a client may send in a burst.
pub const DEFAULT_RATE_BURST: u32 = 60;
/// Default milliseconds for a client to earn one more request (10 per second).
pub const DEFAULT_RATE_REFILL_MS: u64 = 100;
/// Default largest request body outside media uploads.
pub const DEFAULT_BODY_LIMIT: usize = MIB;
/// Default largest media upload.
pub const DEFAULT_BLOB_LIMIT: usize = 64 * MIB;
/// Default seconds before a request is answered with 408.
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
/// Default requests processed at once.
pub const DEFAULT_MAX_CONCURRENCY: usize = 512;
/// How often idle rate-limit entries are forgotten.
const RATE_LIMIT_PRUNE_INTERVAL: Duration = Duration::from_secs(60);
/// HSTS: browsers keep using HTTPS for two years, subdomains included.
const STRICT_TRANSPORT_SECURITY: &str = "max-age=63072000; includeSubDomains";

/// Limits and trust settings, from command-line flags or environment variables.
#[derive(Clone, Debug, clap::Args)]
pub struct Security {
    /// Requests each client may send in a burst before being rate limited.
    #[arg(long, env = "RUPICOLA_RATE_BURST", default_value_t = DEFAULT_RATE_BURST)]
    pub rate_burst: u32,
    /// Milliseconds for a rate-limited client to earn one more request.
    #[arg(long, env = "RUPICOLA_RATE_REFILL_MS", default_value_t = DEFAULT_RATE_REFILL_MS)]
    pub rate_refill_ms: u64,
    /// Largest accepted request body, in bytes, except media uploads.
    #[arg(long, env = "RUPICOLA_BODY_LIMIT", default_value_t = DEFAULT_BODY_LIMIT)]
    pub body_limit: usize,
    /// Largest accepted media file (`PUT /v1/blobs/…`), in bytes.
    #[arg(long, env = "RUPICOLA_BLOB_LIMIT", default_value_t = DEFAULT_BLOB_LIMIT)]
    pub blob_limit: usize,
    /// Seconds a request may take before it is answered with 408.
    #[arg(long, env = "RUPICOLA_REQUEST_TIMEOUT", default_value_t = DEFAULT_REQUEST_TIMEOUT_SECS)]
    pub request_timeout: u64,
    /// Requests processed at once; further requests get 503 instead of queueing.
    #[arg(long, env = "RUPICOLA_MAX_CONCURRENCY", default_value_t = DEFAULT_MAX_CONCURRENCY)]
    pub max_concurrency: usize,
    /// Set when a TLS-terminating reverse proxy (one hop) sits in front: the client
    /// address is then the last X-Forwarded-For entry, and HSTS is sent. Never set
    /// it when clients reach the backend directly, or they can forge their address.
    #[arg(long, env = "RUPICOLA_BEHIND_PROXY")]
    pub behind_proxy: bool,
}

/// Rate-limit key: the client address as seen by us or by our own proxy.
#[derive(Clone)]
pub struct ClientIp {
    behind_proxy: bool,
}

impl KeyExtractor for ClientIp {
    type Key = IpAddr;

    fn extract<T>(&self, request: &Request<T>) -> Result<IpAddr, GovernorError> {
        if self.behind_proxy {
            // The proxy appends the address it saw; earlier entries are client-supplied.
            return request
                .headers()
                .get_all("x-forwarded-for")
                .iter()
                .filter_map(|value| value.to_str().ok())
                .flat_map(|value| value.split(','))
                .next_back()
                .and_then(|last| last.trim().parse().ok())
                .ok_or(GovernorError::UnableToExtractKey);
        }
        request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| address.ip())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// The body limit answers 413 in plain text; give it the same JSON body as every
/// other error.
async fn json_payload_too_large(response: axum::response::Response) -> axum::response::Response {
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"application/json"));
    if response.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE && !is_json {
        return ApiError::PayloadTooLarge.into_response();
    }
    response
}

/// Rate limiter keyed by client address, with idle entries pruned every minute.
pub type RateLimit = GovernorLayer<ClientIp, NoOpMiddleware<QuantaInstant>, axum::body::Body>;

/// Builds a [`RateLimit`]: `burst` requests at once, one more every `refill_ms`.
/// Must run inside a Tokio runtime, which hosts the pruning task.
///
/// # Panics
///
/// Panics if `burst` or `refill_ms` is zero.
pub fn rate_limit(burst: u32, refill_ms: u64, behind_proxy: bool) -> RateLimit {
    let config = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(ClientIp { behind_proxy })
            .per_millisecond(refill_ms)
            .burst_size(burst)
            .finish()
            .expect("rate burst and refill must be greater than zero"),
    );
    let limiter = config.limiter().clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(RATE_LIMIT_PRUNE_INTERVAL);
        loop {
            tick.tick().await;
            limiter.retain_recent();
        }
    });
    GovernorLayer::new(config).error_handler(|error| match error {
        GovernorError::TooManyRequests { wait_time, .. } => ApiError::TooManyRequests {
            retry_after: wait_time,
        }
        .into_response(),
        other => ApiError::Internal(anyhow::anyhow!("rate limiter: {other}")).into_response(),
    })
}

/// Caps request bodies for `router` at `limit` bytes. Applied per group of routes,
/// since media uploads need a much larger cap than everything else.
pub fn limit_body(
    router: Router<crate::state::Shared>,
    limit: usize,
) -> Router<crate::state::Shared> {
    router
        .layer(RequestBodyLimitLayer::new(limit))
        .layer(axum::extract::DefaultBodyLimit::max(limit))
}

/// Wraps `router` with every protection except body limits (see [`limit_body`]).
/// Must run inside a Tokio runtime.
pub fn harden(router: Router, settings: &Security) -> Router {
    let mut headers = vec![
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "no-referrer"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; frame-ancestors 'none'",
        ),
        (header::CACHE_CONTROL, "no-store"),
        (
            HeaderName::from_static("cross-origin-resource-policy"),
            "same-origin",
        ),
    ];
    if settings.behind_proxy {
        headers.push((header::STRICT_TRANSPORT_SECURITY, STRICT_TRANSPORT_SECURITY));
    }
    let mut router = router
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|error: tower::BoxError| async move {
                    if error.is::<tower::timeout::error::Elapsed>() {
                        ApiError::Timeout
                    } else if error.is::<tower::load_shed::error::Overloaded>() {
                        ApiError::Overloaded
                    } else {
                        ApiError::Internal(anyhow::anyhow!(error))
                    }
                }))
                .load_shed()
                .concurrency_limit(settings.max_concurrency)
                .timeout(Duration::from_secs(settings.request_timeout)),
        )
        .layer(axum::middleware::map_response(json_payload_too_large))
        .layer(rate_limit(
            settings.rate_burst,
            settings.rate_refill_ms,
            settings.behind_proxy,
        ))
        .layer(CatchPanicLayer::custom(|_| {
            ApiError::Internal(anyhow::anyhow!("handler panicked")).into_response()
        }));
    for (name, value) in headers {
        router = router.layer(SetResponseHeaderLayer::overriding(
            name,
            HeaderValue::from_static(value),
        ));
    }
    let request_id = HeaderName::from_static("x-request-id");
    router
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(TraceLayer::new_for_http())
        .layer(SetSensitiveRequestHeadersLayer::new([
            header::AUTHORIZATION,
            header::COOKIE,
        ]))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
}
