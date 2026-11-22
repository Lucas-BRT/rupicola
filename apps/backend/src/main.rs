//! Rupicola backend: the API server every client (web panel, CLI, desktop GUI) talks to.
//! It is the only process that touches the database and the media store.

mod auth;
mod db;
mod error;
mod projects;
mod security;
mod state;
#[cfg(test)]
mod testkit;

use anyhow::{Context, bail};
use axum::{Json, Router, routing};
use clap::Parser;
use error::ApiError;
use rupicola_api::{Health, HealthStatus};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::PathBuf,
    time::Duration,
};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Bind IPv4 address.
    #[arg(long, env = "HOST", default_value_t = Ipv4Addr::LOCALHOST)]
    bind: Ipv4Addr,
    /// Listen port.
    #[arg(long, env = "PORT", default_value_t = 4730)]
    port: u16,
    /// Database file; media blobs live next to it.
    #[arg(long, env = "RUPICOLA_DB", default_value = ".rupicola/panel.db")]
    db: PathBuf,
    /// Code required to create the first administrator (at least 32 characters). Without
    /// it, setup is only allowed while the backend listens on loopback.
    #[arg(long, env = "RUPICOLA_SETUP_CODE", hide_env_values = true)]
    setup_code: Option<String>,
    /// Probe the local listener and exit; used by container health checks.
    #[arg(long)]
    healthcheck: bool,
    #[command(flatten)]
    security: security::Security,
}

/// Every route, wrapped in the request hardening from [`security`].
fn app(settings: &security::Security, state: state::Shared) -> Router {
    // Password guessing gets a much smaller budget than ordinary API use.
    let sign_in_limit = security::rate_limit(5, 12_000, settings.behind_proxy);
    let credentials = Router::new()
        .route("/v1/sessions", routing::post(auth::sign_in))
        .route("/v1/setup", routing::post(auth::setup))
        .route_layer(sign_in_limit);
    let routes = Router::new()
        .route("/healthz", routing::get(health))
        .route("/v1/me", routing::get(auth::me))
        .route("/v1/sessions/current", routing::delete(auth::sign_out))
        .route(
            "/v1/projects",
            routing::get(projects::list).post(projects::create),
        )
        .route(
            "/v1/projects/{id}",
            routing::get(projects::get)
                .patch(projects::update)
                .delete(projects::delete),
        )
        .route("/v1/projects/{id}/members", routing::get(projects::members))
        .route(
            "/v1/projects/{id}/members/{handle}",
            routing::put(projects::put_member).delete(projects::delete_member),
        )
        .route("/v1/projects/{id}/audit", routing::get(projects::audit_log))
        .merge(credentials)
        .fallback(|| async { ApiError::NotFound })
        .with_state(state);
    security::harden(routes, settings)
}

async fn health() -> Json<Health> {
    Json(Health {
        status: HealthStatus::Ok,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.healthcheck {
        let response = fetch(args.port, "/healthz").context("backend is not reachable")?;
        if !response.starts_with("HTTP/1.1 200") {
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
    if setup_code.is_some_and(|code| code.len() < 32) {
        bail!("RUPICOLA_SETUP_CODE must contain at least 32 characters");
    }
    let state = state::AppState::open(&args.db, setup_code, args.bind.is_loopback()).await?;
    let listener = tokio::net::TcpListener::bind((args.bind, args.port))
        .await
        .with_context(|| format!("cannot listen on {}:{}", args.bind, args.port))?;
    tracing::info!(
        address = %listener.local_addr()?,
        behind_proxy = args.security.behind_proxy,
        "rupicola-backend listening"
    );
    axum::serve(
        listener,
        app(&args.security, state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown())
    .await
    .context("server stopped with an error")?;
    tracing::info!("rupicola-backend stopped");
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

fn send(port: u16, request: &str) -> std::io::Result<String> {
    let timeout = Duration::from_secs(3);
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

    #[tokio::test]
    async fn healthz_reports_ok_with_the_package_version() {
        let port = start(settings()).await;
        let response = get(port, "/healthz", "").await;

        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let health: Health = serde_json::from_str(body(&response)).unwrap();
        assert_eq!(health.status, HealthStatus::Ok);
        assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn unknown_routes_are_not_found() {
        let port = start(settings()).await;
        let response = get(port, "/v1/nothing", "").await;

        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
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
        assert_eq!(header(&response, "x-request-id").map(str::len), Some(36));
    }

    #[tokio::test]
    async fn clients_over_the_burst_are_rate_limited() {
        let port = start(security::Security {
            rate_burst: 2,
            rate_refill_ms: 60_000,
            ..settings()
        })
        .await;
        for _ in 0..2 {
            assert!(get(port, "/healthz", "").await.starts_with("HTTP/1.1 200"));
        }
        let response = get(port, "/healthz", "").await;

        assert!(response.starts_with("HTTP/1.1 429"), "{response}");
        assert!(header(&response, "retry-after").is_some());
        let error: rupicola_api::ErrorBody = serde_json::from_str(body(&response)).unwrap();
        assert_eq!(error.code, "rate-limited");
    }

    #[tokio::test]
    async fn behind_a_proxy_the_last_forwarded_address_is_the_client() {
        let port = start(security::Security {
            rate_burst: 1,
            rate_refill_ms: 60_000,
            behind_proxy: true,
            ..settings()
        })
        .await;
        let from = |address: &str| format!("X-Forwarded-For: 203.0.113.9, {address}\r\n");

        let first = get(port, "/healthz", &from("198.51.100.1")).await;
        assert!(first.starts_with("HTTP/1.1 200"), "{first}");
        // A forged first entry does not buy a fresh budget.
        let forged = get(
            port,
            "/healthz",
            "X-Forwarded-For: 192.0.2.77, 198.51.100.1\r\n",
        )
        .await;
        assert!(forged.starts_with("HTTP/1.1 429"), "{forged}");
        let other = get(port, "/healthz", &from("198.51.100.2")).await;
        assert!(other.starts_with("HTTP/1.1 200"), "{other}");
        assert!(header(&other, "strict-transport-security").is_some());
    }

    #[tokio::test]
    async fn oversized_bodies_are_rejected_before_reaching_handlers() {
        let port = start(security::Security {
            body_limit: 1024,
            ..settings()
        })
        .await;
        let response = request(
            port,
            "POST /healthz HTTP/1.1\r\nHost: test\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;

        assert!(response.starts_with("HTTP/1.1 413"), "{response}");
        let error: rupicola_api::ErrorBody = serde_json::from_str(body(&response)).unwrap();
        assert_eq!(error.code, "payload-too-large");
    }
    const OWNER: &str = r#"{"handle":"Ana","password":"correct horse battery"}"#;

    #[tokio::test]
    async fn the_first_owner_is_created_once_and_signed_in() {
        let port = start(settings()).await;
        let created = call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        assert!(created.starts_with("HTTP/1.1 201"), "{created}");

        let me = call(port, "GET", "/v1/me", Some(&token_of(&created)), None).await;
        let me: rupicola_api::Me = serde_json::from_str(body(&me)).unwrap();
        assert_eq!(me.kind, rupicola_api::TokenKind::Session);
        assert_eq!(me.handle.as_deref(), Some("ana"));
        assert!(me.admin);

        let again = call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        assert!(again.starts_with("HTTP/1.1 409"), "{again}");
    }

    #[tokio::test]
    async fn setup_needs_the_code_unless_listening_on_loopback() {
        let code = "a-setup-code-that-is-long-enough-123";
        let port = start_custom(settings(), Some(code), false).await.0;
        let without = call(port, "POST", "/v1/setup", None, Some(OWNER)).await;
        assert!(without.starts_with("HTTP/1.1 403"), "{without}");
        let with =
            format!(r#"{{"handle":"ana","password":"correct horse battery","code":"{code}"}}"#);
        let created = call(port, "POST", "/v1/setup", None, Some(&with)).await;
        assert!(created.starts_with("HTTP/1.1 201"), "{created}");
    }

    #[tokio::test]
    async fn sign_in_sign_out_and_rejected_credentials() {
        let port = start(settings()).await;
        call(port, "POST", "/v1/setup", None, Some(OWNER)).await;

        let wrong = r#"{"handle":"ana","password":"wrong password here"}"#;
        let refused = call(port, "POST", "/v1/sessions", None, Some(wrong)).await;
        assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");
        let unknown = r#"{"handle":"nobody","password":"correct horse battery"}"#;
        let refused = call(port, "POST", "/v1/sessions", None, Some(unknown)).await;
        assert!(refused.starts_with("HTTP/1.1 401"), "{refused}");

        let signed_in = call(port, "POST", "/v1/sessions", None, Some(OWNER)).await;
        assert!(signed_in.starts_with("HTTP/1.1 201"), "{signed_in}");
        let token = token_of(&signed_in);
        let out = call(port, "DELETE", "/v1/sessions/current", Some(&token), None).await;
        assert!(out.starts_with("HTTP/1.1 204"), "{out}");
        let after = call(port, "GET", "/v1/me", Some(&token), None).await;
        assert!(after.starts_with("HTTP/1.1 401"), "{after}");

        for bad in [None, Some("not-a-token"), Some(&"0".repeat(64)[..])] {
            let response = call(port, "GET", "/v1/me", bad, None).await;
            assert!(response.starts_with("HTTP/1.1 401"), "{response}");
        }
    }

    #[tokio::test]
    async fn password_guessing_is_rate_limited_per_client() {
        let port = start(settings()).await;
        let wrong = r#"{"handle":"ana","password":"wrong password here"}"#;
        for _ in 0..5 {
            call(port, "POST", "/v1/sessions", None, Some(wrong)).await;
        }
        let limited = call(port, "POST", "/v1/sessions", None, Some(wrong)).await;
        assert!(limited.starts_with("HTTP/1.1 429"), "{limited}");
        // Ordinary API use keeps its own, larger budget.
        assert!(get(port, "/healthz", "").await.starts_with("HTTP/1.1 200"));
    }
}
