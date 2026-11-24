use std::{net::Ipv4Addr, sync::Arc};

use anyhow::{Context as _, Result, bail};
use clap::Parser;
use http::{StatusCode, header};
use rupicola_api::Capture;
use rupicola_client::{Api, HttpApi};
use serde::Deserialize;
use tokio::net::TcpListener;
use topcoat::router::{
    Body, Router,
    content::Html,
    request::Request,
    response::{IntoResponse, Response},
};
use topcoat::{
    context::Cx,
    view::{ViewExt, view},
};

mod app;
mod auth;
mod backend;
mod components;
mod media;
mod project;
mod records;
mod runs;
mod systems;
mod views;
use auth::Account;

type PageResult<T> = std::result::Result<T, StatusCode>;

const DEFAULT_PORT: u16 = 4731;
const HEALTHCHECK_TIMEOUT_SECS: u64 = 3;
const HEALTHCHECK_BUF_BYTES: usize = 64;

#[derive(Parser)]
#[command(about = "Browse local Rupicola visual evidence")]
struct Args {
    /// Bind IPv4 address (defaults to HOST env var or 127.0.0.1).
    #[arg(long)]
    bind: Option<Ipv4Addr>,
    #[arg(long)]
    port: Option<u16>,
    /// Public HTTPS origin when running behind a reverse proxy (or `RUPICOLA_PUBLIC_ORIGIN`).
    #[arg(long)]
    public_origin: Option<String>,
    /// Base URL of the Rupicola backend that owns accounts and projects (or `RUPICOLA_SERVER_URL`).
    #[arg(long, env = "RUPICOLA_SERVER_URL")]
    server_url: Option<String>,
    /// Run a self-contained health probe against the local listener and exit.
    #[arg(long)]
    healthcheck: bool,
}

struct Panel {
    origin: String,
    localhost_origin: String,
    host: String,
    secure: bool,
    /// The backend, which owns accounts, sessions and project membership.
    api: Arc<dyn Api>,
}

struct Run {
    key: String,
    project_id: String,
    project: String,
    scenario: String,
    status: String,
    inspection: String,
    created_at_ms: i64,
    capture: Capture,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let configured_port = args
        .port
        .or_else(|| std::env::var("PORT").ok().and_then(|p| p.parse().ok()))
        .unwrap_or(DEFAULT_PORT);

    if args.healthcheck {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from((Ipv4Addr::LOCALHOST, configured_port)),
            std::time::Duration::from_secs(HEALTHCHECK_TIMEOUT_SECS),
        )?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(
            HEALTHCHECK_TIMEOUT_SECS,
        )))?;
        write!(
            stream,
            "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{configured_port}\r\nConnection: close\r\n\r\n"
        )?;
        let mut buf = [0u8; HEALTHCHECK_BUF_BYTES];
        let n = stream.read(&mut buf)?;
        let head = String::from_utf8_lossy(&buf[..n]);
        if head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.1 303") {
            return Ok(());
        }
        bail!("healthcheck failed");
    }

    let server_url = args
        .server_url
        .context("the panel needs the backend: pass --server-url or set RUPICOLA_SERVER_URL")?;
    let api: Arc<dyn Api> = Arc::new(HttpApi::new(&server_url)?);
    let bind_ip = args
        .bind
        .or_else(|| std::env::var("HOST").ok().and_then(|h| h.parse().ok()))
        .unwrap_or(Ipv4Addr::LOCALHOST);
    let listener = TcpListener::bind((bind_ip, configured_port)).await?;
    let port = listener.local_addr()?.port();
    let public_origin = args.public_origin.or_else(|| {
        std::env::var("RUPICOLA_PUBLIC_ORIGIN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    });
    let secure = public_origin.is_some();
    let origin = if let Some(origin) = public_origin {
        let parsed = url::Url::parse(&origin)?;
        if parsed.scheme() != "https"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            bail!("--public-origin must be an HTTPS origin without a path or credentials");
        }
        parsed.origin().ascii_serialization()
    } else {
        format!("http://127.0.0.1:{port}")
    };
    let host = origin
        .split_once("://")
        .context("invalid origin")?
        .1
        .to_owned();
    let panel = Arc::new(Panel {
        origin: origin.clone(),
        localhost_origin: format!("http://localhost:{port}"),
        secure,
        host,
        api,
    });
    let app = app::router(panel);
    tracing::info!(%bind_ip, port, %origin, "listening");
    topcoat::serve(listener, app)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(())
}

fn redirect(url: &str) -> Response {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, url)
        .body(Body::empty())
        .unwrap_or_else(|err| {
            tracing::error!(%err, "invalid redirect location");
            status(StatusCode::INTERNAL_SERVER_ERROR)
        })
}

fn status(code: StatusCode) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = code;
    response
}

fn render_error(error: impl std::fmt::Display) -> StatusCode {
    tracing::error!(%error, "panel render failed");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
