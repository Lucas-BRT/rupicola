//! Rupicola backend: the API server every client (web panel, CLI, desktop GUI) talks to.
//! It is the only process that touches the database and the media store.

mod error;
mod security;

use anyhow::{Context, bail};
use axum::{Json, Router, routing};
use clap::Parser;
use error::ApiError;
use rupicola_api::{Health, HealthStatus};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
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
    /// Probe the local listener and exit; used by container health checks.
    #[arg(long)]
    healthcheck: bool,
    #[command(flatten)]
    security: security::Security,
}

/// Every route, wrapped in the request hardening from [`security`].
fn app(settings: &security::Security) -> Router {
    let routes = Router::new()
        .route("/healthz", routing::get(health))
        .fallback(|| async { ApiError::NotFound });
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
        app(&args.security).into_make_service_with_connect_info::<SocketAddr>(),
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

    fn settings() -> security::Security {
        security::Security {
            rate_burst: 60,
            rate_refill_ms: 100,
            body_limit: 1024,
            request_timeout: 30,
            max_concurrency: 16,
            behind_proxy: false,
        }
    }

    async fn start(settings: security::Security) -> u16 {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = app(&settings).into_make_service_with_connect_info::<SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await });
        port
    }

    async fn request(port: u16, raw: String) -> String {
        tokio::task::spawn_blocking(move || send(port, &raw))
            .await
            .unwrap()
            .unwrap()
    }

    async fn get(port: u16, path: &str, extra_headers: &str) -> String {
        request(
            port,
            format!(
                "GET {path} HTTP/1.1\r\nHost: test\r\n{extra_headers}Connection: close\r\n\r\n"
            ),
        )
        .await
    }

    fn body(response: &str) -> &str {
        response.split_once("\r\n\r\n").unwrap().1
    }

    fn header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
        response.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }

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
        let port = start(settings()).await;
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
}
