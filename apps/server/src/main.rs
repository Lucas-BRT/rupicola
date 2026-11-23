//! Rupicola server: the API server every client (web panel, CLI, desktop GUI) talks to.
//! It is the only process that touches the database and the media store.

mod config;
mod design_system;
mod domain;
mod fixes;
mod http;
mod lineage;
mod problems;
mod publish;
mod removal;
mod repositories;
mod search;
mod selections;
mod services;
mod state;
mod storage;
#[cfg(test)]
mod testkit;
mod topics;

use anyhow::{Context, bail};
use clap::Parser;
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::PathBuf,
    time::Duration,
};
use tracing_subscriber::EnvFilter;

/// Port the server listens on unless told otherwise.
const DEFAULT_PORT: u16 = 4730;
/// Shortest accepted setup code, so it cannot be guessed.
const MIN_SETUP_CODE_LEN: usize = 32;
/// How long the health probe waits to connect and to read the answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Bind IPv4 address.
    #[arg(long, env = "HOST", default_value_t = Ipv4Addr::LOCALHOST)]
    bind: Ipv4Addr,
    /// Listen port.
    #[arg(long, env = "PORT", default_value_t = DEFAULT_PORT)]
    port: u16,
    /// Database file; media blobs live next to it.
    #[arg(long, env = "RUPICOLA_DB", default_value = ".rupicola/panel.db")]
    db: PathBuf,
    /// Code required to create the first administrator; short codes are refused.
    /// Without it, setup is only allowed while the server listens on loopback.
    #[arg(long, env = "RUPICOLA_SETUP_CODE", hide_env_values = true)]
    setup_code: Option<String>,
    /// Probe the local listener and exit; used by container health checks.
    #[arg(long)]
    healthcheck: bool,
    #[command(flatten)]
    security: http::security::Security,
    #[command(flatten)]
    uploads: config::UploadLimits,
    /// Whether any project may remove a capture that something cites; `never` overrides
    /// every project's policy.
    #[arg(long, env = "RUPICOLA_CITED_DELETION", value_enum, default_value_t)]
    cited_deletion: removal::CitedDeletion,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.healthcheck {
        let response = fetch(args.port, http::HEALTH_PATH).context("server is not reachable")?;
        if status_code(&response) != Some(axum::http::StatusCode::OK) {
            bail!("healthcheck failed");
        }
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=debug")),
        )
        .init();
    let setup_code = args.setup_code.as_deref().filter(|code| !code.is_empty());
    if setup_code.is_some_and(|code| code.len() < MIN_SETUP_CODE_LEN) {
        bail!("RUPICOLA_SETUP_CODE must contain at least {MIN_SETUP_CODE_LEN} characters");
    }
    let state = state::AppState::open(
        &args.db,
        setup_code,
        args.bind.is_loopback(),
        domain::limits::LimitDefaults::new(
            args.security.blob_limit,
            args.security.body_limit,
            &args.uploads,
        ),
    )
    .await?;
    state.cited_deletion.store(
        args.cited_deletion == removal::CitedDeletion::Allow,
        std::sync::atomic::Ordering::Relaxed,
    );
    let listener = tokio::net::TcpListener::bind((args.bind, args.port))
        .await
        .with_context(|| format!("cannot listen on {}:{}", args.bind, args.port))?;
    tracing::info!(
        address = %listener.local_addr()?,
        behind_proxy = args.security.behind_proxy,
        "rupicola-server listening"
    );
    axum::serve(
        listener,
        http::app(&args.security, state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await
    .context("server stopped with an error")?;
    tracing::info!("rupicola-server stopped");
    Ok(())
}

/// Resolves on Ctrl+C or, on Unix, SIGTERM (what `docker stop` sends).
async fn shutdown() {
    #[cfg(unix)]
    {
        let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            let _ = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}

/// Minimal blocking HTTP/1.1 GET against the local listener, returning the raw
/// response. Keeps the health probe free of an HTTP client dependency.
fn fetch(port: u16, path: &str) -> std::io::Result<String> {
    send(
        port,
        &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"),
    )
}

/// Status code of a raw HTTP/1.1 response.
fn status_code(response: &str) -> Option<axum::http::StatusCode> {
    let code = response.split(' ').nth(1)?.parse::<u16>().ok()?;
    axum::http::StatusCode::from_u16(code).ok()
}

fn send(port: u16, request: &str) -> std::io::Result<String> {
    let timeout = PROBE_TIMEOUT;
    let mut stream =
        TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.write_all(request.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{Health, HealthStatus};

    /// Length of a hyphenated UUID, the format of request ids.
    const UUID_TEXT_LEN: usize = 36;
    /// A body cap small enough to exceed with a short request.
    const TINY_BODY_LIMIT: usize = 1024;
    /// A declared body well over [`TINY_BODY_LIMIT`].
    const OVERSIZED_BODY: usize = TINY_BODY_LIMIT * 4;
    /// Requests allowed before the rate limit tests expect a 429.
    const SMALL_BURST: u32 = 2;
    const SINGLE_REQUEST: u32 = 1;

    #[tokio::test]
    async fn healthz_reports_ok_with_the_package_version() {
        let port = start(settings()).await;
        let response = get(port, "/healthz", "").await;

        assert_eq!(status(&response), StatusCode::OK, "{response}");
        let health: Health = serde_json::from_str(body(&response)).unwrap();
        assert_eq!(health.status, HealthStatus::Ok);
        assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn unknown_routes_are_not_found() {
        let port = start(settings()).await;
        let response = get(port, "/v1/nothing", "").await;

        assert_eq!(status(&response), StatusCode::NOT_FOUND, "{response}");
        let error: rupicola_api::ErrorBody = serde_json::from_str(body(&response)).unwrap();
        assert_eq!(error.code, "not-found");
    }

    #[tokio::test]
    async fn responses_carry_security_headers_and_a_request_id() {
        let port = start(settings()).await;
        let response = get(port, "/healthz", "").await;

        assert_eq!(header(&response, "x-content-type-options"), Some("nosniff"));
        assert_eq!(header(&response, "x-frame-options"), Some("DENY"));
        assert_eq!(header(&response, "cache-control"), Some("no-store"));
        assert!(header(&response, "content-security-policy").is_some());
        assert!(header(&response, "strict-transport-security").is_none());
        assert_eq!(
            header(&response, "x-request-id").map(str::len),
            Some(UUID_TEXT_LEN)
        );
    }

    #[tokio::test]
    async fn clients_over_the_burst_are_rate_limited() {
        let port = start(http::security::Security {
            rate_burst: SMALL_BURST,
            rate_refill_ms: NO_REFILL_MS,
            ..settings()
        })
        .await;
        for _ in 0..SMALL_BURST {
            assert_eq!(status(&get(port, "/healthz", "").await), StatusCode::OK);
        }
        let response = get(port, "/healthz", "").await;

        assert_eq!(
            status(&response),
            StatusCode::TOO_MANY_REQUESTS,
            "{response}"
        );
        assert!(header(&response, "retry-after").is_some());
        let error: rupicola_api::ErrorBody = serde_json::from_str(body(&response)).unwrap();
        assert_eq!(error.code, "rate-limited");
    }

    #[tokio::test]
    async fn behind_a_proxy_the_last_forwarded_address_is_the_client() {
        let port = start(http::security::Security {
            rate_burst: SINGLE_REQUEST,
            rate_refill_ms: NO_REFILL_MS,
            behind_proxy: true,
            ..settings()
        })
        .await;
        let from = |address: &str| format!("X-Forwarded-For: 203.0.113.9, {address}\r\n");

        let first = get(port, "/healthz", &from("198.51.100.1")).await;
        assert_eq!(status(&first), StatusCode::OK, "{first}");
        // A forged first entry does not buy a fresh budget.
        let forged = get(
            port,
            "/healthz",
            "X-Forwarded-For: 192.0.2.77, 198.51.100.1\r\n",
        )
        .await;
        assert_eq!(status(&forged), StatusCode::TOO_MANY_REQUESTS, "{forged}");
        let other = get(port, "/healthz", &from("198.51.100.2")).await;
        assert_eq!(status(&other), StatusCode::OK, "{other}");
        assert!(header(&other, "strict-transport-security").is_some());
    }

    #[tokio::test]
    async fn oversized_bodies_are_rejected_before_reaching_handlers() {
        let port = start(http::security::Security {
            body_limit: TINY_BODY_LIMIT,
            ..settings()
        })
        .await;
        let response = request(
            port,
            format!(
                "POST /v1/projects HTTP/1.1\r\nHost: test\r\nContent-Length: {OVERSIZED_BODY}\r\n\
                 Connection: close\r\n\r\n"
            ),
        )
        .await;

        assert_eq!(
            status(&response),
            StatusCode::PAYLOAD_TOO_LARGE,
            "{response}"
        );
        let error: rupicola_api::ErrorBody = serde_json::from_str(body(&response)).unwrap();
        assert_eq!(error.code, "payload-too-large");
    }
    const OWNER: &str = r#"{"handle":"Ana","password":"correct horse battery"}"#;

    #[tokio::test]
    async fn the_first_owner_is_created_once_and_signed_in() {
        let port = start(settings()).await;
        let created = call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");

        let me = call(port, "GET", "/v1/me", Some(&token_of(&created)), None).await;
        let me: rupicola_api::Me = serde_json::from_str(body(&me)).unwrap();
        assert_eq!(me.kind, rupicola_api::TokenKind::Session);
        assert_eq!(me.handle.as_deref(), Some("ana"));
        assert!(me.admin);

        let again = call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        assert_eq!(status(&again), StatusCode::CONFLICT, "{again}");
    }

    #[tokio::test]
    async fn setup_is_reported_as_required_until_the_first_owner_exists() {
        let port = start(settings()).await;
        let before = call(port, "GET", "/v1/setup", None, None).await;
        assert_eq!(status(&before), StatusCode::OK, "{before}");
        assert!(json::<rupicola_api::SetupStatus>(&before).required);

        call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        let after = call(port, "GET", "/v1/setup", None, None).await;
        assert!(!json::<rupicola_api::SetupStatus>(&after).required);
    }

    #[tokio::test]
    async fn setup_needs_the_code_unless_listening_on_loopback() {
        let code = "a-setup-code-that-is-long-enough-123";
        let port = start_custom(settings(), Some(code), false).await.0;
        let without = call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        assert_eq!(status(&without), StatusCode::FORBIDDEN, "{without}");
        let with =
            format!(r#"{{"handle":"ana","password":"correct horse battery","code":"{code}"}}"#);
        let created = call(port, "POST", "/v1/setup", None, Some(&with)).await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
    }

    #[tokio::test]
    async fn sign_in_sign_out_and_rejected_credentials() {
        let port = start(settings()).await;
        call(port, "POST", "/v1/setup", None, Some(OWNER)).await;

        let wrong = r#"{"handle":"ana","password":"wrong password here"}"#;
        let refused = call(port, "POST", "/v1/sessions", None, Some(wrong)).await;
        assert_eq!(status(&refused), StatusCode::UNAUTHORIZED, "{refused}");
        let unknown = r#"{"handle":"nobody","password":"correct horse battery"}"#;
        let refused = call(port, "POST", "/v1/sessions", None, Some(unknown)).await;
        assert_eq!(status(&refused), StatusCode::UNAUTHORIZED, "{refused}");

        let signed_in = call(port, "POST", "/v1/sessions", None, Some(OWNER)).await;
        assert_eq!(status(&signed_in), StatusCode::CREATED, "{signed_in}");
        let token = token_of(&signed_in);
        let out = call(port, "DELETE", "/v1/sessions/current", Some(&token), None).await;
        assert_eq!(status(&out), StatusCode::NO_CONTENT, "{out}");
        let after = call(port, "GET", "/v1/me", Some(&token), None).await;
        assert_eq!(status(&after), StatusCode::UNAUTHORIZED, "{after}");

        for bad in [
            None,
            Some("not-a-token"),
            Some(&"0".repeat(crate::domain::secret::HEX_SECRET_LEN)[..]),
        ] {
            let response = call(port, "GET", "/v1/me", bad, None).await;
            assert_eq!(status(&response), StatusCode::UNAUTHORIZED, "{response}");
        }
    }

    #[tokio::test]
    async fn changing_the_password_ends_the_other_sessions_and_checks_the_current_one() {
        let port = start(settings()).await;
        call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        let first = token_of(&call(port, "POST", "/v1/sessions", None, Some(OWNER)).await);
        let second = token_of(&call(port, "POST", "/v1/sessions", None, Some(OWNER)).await);

        let wrong = r#"{"current":"not the password","new":"another long password"}"#;
        let refused = call(port, "PUT", "/v1/me/password", Some(&first), Some(wrong)).await;
        assert_eq!(
            status(&refused),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused}"
        );
        let short = r#"{"current":"correct horse battery","new":"short"}"#;
        let refused = call(port, "PUT", "/v1/me/password", Some(&first), Some(short)).await;
        assert_eq!(
            status(&refused),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused}"
        );

        let change = r#"{"current":"correct horse battery","new":"another long password"}"#;
        let changed = call(port, "PUT", "/v1/me/password", Some(&first), Some(change)).await;
        assert_eq!(status(&changed), StatusCode::NO_CONTENT, "{changed}");
        let kept = call(port, "GET", "/v1/me", Some(&first), None).await;
        assert_eq!(status(&kept), StatusCode::OK, "{kept}");
        let ended = call(port, "GET", "/v1/me", Some(&second), None).await;
        assert_eq!(status(&ended), StatusCode::UNAUTHORIZED, "{ended}");

        let old = call(port, "POST", "/v1/sessions", None, Some(OWNER)).await;
        assert_eq!(status(&old), StatusCode::UNAUTHORIZED, "{old}");
        let new = r#"{"handle":"ana","password":"another long password"}"#;
        let signed_in = call(port, "POST", "/v1/sessions", None, Some(new)).await;
        assert_eq!(status(&signed_in), StatusCode::CREATED, "{signed_in}");
    }

    #[tokio::test]
    async fn signing_out_the_others_keeps_the_current_session() {
        let port = start(settings()).await;
        call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        let first = token_of(&call(port, "POST", "/v1/sessions", None, Some(OWNER)).await);
        let second = token_of(&call(port, "POST", "/v1/sessions", None, Some(OWNER)).await);
        let out = call(port, "DELETE", "/v1/sessions/others", Some(&first), None).await;
        assert_eq!(status(&out), StatusCode::NO_CONTENT, "{out}");
        let kept = call(port, "GET", "/v1/me", Some(&first), None).await;
        assert_eq!(status(&kept), StatusCode::OK, "{kept}");
        let ended = call(port, "GET", "/v1/me", Some(&second), None).await;
        assert_eq!(status(&ended), StatusCode::UNAUTHORIZED, "{ended}");
    }

    #[tokio::test]
    async fn password_guessing_is_rate_limited_per_client() {
        let port = start(settings()).await;
        let wrong = r#"{"handle":"ana","password":"wrong password here"}"#;
        for _ in 0..http::SIGN_IN_BURST {
            call(port, "POST", "/v1/sessions", None, Some(wrong)).await;
        }
        let limited = call(port, "POST", "/v1/sessions", None, Some(wrong)).await;
        assert_eq!(status(&limited), StatusCode::TOO_MANY_REQUESTS, "{limited}");
        // Ordinary API use keeps its own, larger budget.
        assert_eq!(status(&get(port, "/healthz", "").await), StatusCode::OK);
    }
}
