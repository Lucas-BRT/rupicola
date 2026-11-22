//! Rupicola backend: the API server every client (web panel, CLI, desktop GUI) talks to.
//! It is the only process that touches the database and the media store.

mod error;

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
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

const DEFAULT_PORT: u16 = 4730;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Bind IPv4 address (defaults to the HOST variable, then 127.0.0.1).
    #[arg(long)]
    bind: Option<Ipv4Addr>,
    /// Listen port (defaults to the PORT variable, then 4730).
    #[arg(long)]
    port: Option<u16>,
    /// Probe the local listener and exit; used by container health checks.
    #[arg(long)]
    healthcheck: bool,
}

fn router() -> Router {
    Router::new()
        .route("/healthz", routing::get(health))
        .fallback(|| async { ApiError::NotFound })
        .layer(TraceLayer::new_for_http())
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
    let port = args
        .port
        .or_else(|| variable("PORT"))
        .unwrap_or(DEFAULT_PORT);
    if args.healthcheck {
        let response = fetch(port, "/healthz").context("backend is not reachable")?;
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
    let bind = args
        .bind
        .or_else(|| variable("HOST"))
        .unwrap_or(Ipv4Addr::LOCALHOST);
    let listener = tokio::net::TcpListener::bind((bind, port))
        .await
        .with_context(|| format!("cannot listen on {bind}:{port}"))?;
    tracing::info!(address = %listener.local_addr()?, "rupicola-backend listening");
    axum::serve(listener, router())
        .with_graceful_shutdown(shutdown())
        .await
        .context("server stopped with an error")?;
    tracing::info!("rupicola-backend stopped");
    Ok(())
}

fn variable<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.trim().parse().ok()
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
    let timeout = Duration::from_secs(3);
    let mut stream =
        TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn healthz_reports_ok_with_the_package_version() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, router()).await });

        let response = tokio::task::spawn_blocking(move || fetch(port, "/healthz"))
            .await
            .unwrap()
            .unwrap();

        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let body = response.split_once("\r\n\r\n").unwrap().1;
        let health: Health = serde_json::from_str(body).unwrap();
        assert_eq!(health.status, HealthStatus::Ok);
        assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn unknown_routes_are_not_found() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, router()).await });

        let response = tokio::task::spawn_blocking(move || fetch(port, "/v1/nothing"))
            .await
            .unwrap()
            .unwrap();

        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
        let body = response.split_once("\r\n\r\n").unwrap().1;
        let error: rupicola_api::ErrorBody = serde_json::from_str(body).unwrap();
        assert_eq!(error.code, "not-found");
    }
}
