use std::{error::Error, fs, net::Ipv4Addr, path::PathBuf, sync::Arc};

use clap::Parser;
use http::{StatusCode, header};
use rupicola_client::{Api, HttpApi};
use rupicola_core::capture_manifest::Manifest;
use serde::Deserialize;
use tokio::{net::TcpListener, sync::Mutex};
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
use turso::Connection;

mod app;
mod auth;
mod backend;
mod components;
mod db;
mod design_system;
mod media;
mod project;
mod records;
mod runs;
mod views;
use auth::Account;
use rupicola_core::{design_record, fix_record, problem_record};

type BoxError = Box<dyn Error + Send + Sync>;
type PageResult<T> = Result<T, StatusCode>;

#[derive(Parser)]
#[command(about = "Browse local Rupicola visual evidence")]
struct Args {
    /// Local, rebuildable index file.
    #[arg(long, default_value = ".rupicola/panel.db")]
    db: PathBuf,
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
    index: Mutex<Connection>,
    origin: String,
    localhost_origin: String,
    host: String,
    secure: bool,
    /// The backend, which owns accounts, sessions and project membership.
    api: Arc<dyn Api>,
    design_validation: tokio::sync::Semaphore,
}

struct Run {
    key: String,
    project_id: String,
    project: String,
    scenario: String,
    status: String,
    inspection: String,
    created_at_ms: i64,
    manifest: Manifest,
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let args = Args::parse();
    let configured_port = args
        .port
        .or_else(|| std::env::var("PORT").ok().and_then(|p| p.parse().ok()))
        .unwrap_or(4731);

    if args.healthcheck {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from((Ipv4Addr::LOCALHOST, configured_port)),
            std::time::Duration::from_secs(3),
        )?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(3)))?;
        write!(
            stream,
            "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{configured_port}\r\nConnection: close\r\n\r\n"
        )?;
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf)?;
        let head = String::from_utf8_lossy(&buf[..n]);
        if head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.1 303") {
            return Ok(());
        }
        return Err("healthcheck failed".into());
    }

    let server_url = args
        .server_url
        .ok_or("the panel needs the backend: pass --server-url or set RUPICOLA_SERVER_URL")?;
    let api: Arc<dyn Api> = Arc::new(HttpApi::new(&server_url)?);
    let db_path = std::env::var_os("RUPICOLA_DB")
        .map(PathBuf::from)
        .unwrap_or(args.db);
    if let Some(parent) = db_path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let database = turso::Builder::new_local(db_path.to_str().ok_or("invalid database path")?)
        .build()
        .await?;
    let index = database.connect()?;
    db::migrate(&index).await?;
    design_system::init(&index).await?;
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
            return Err(
                "--public-origin must be an HTTPS origin without a path or credentials".into(),
            );
        }
        parsed.origin().ascii_serialization()
    } else {
        format!("http://127.0.0.1:{port}")
    };
    let host = origin
        .split_once("://")
        .ok_or("invalid origin")?
        .1
        .to_owned();
    let panel = Arc::new(Panel {
        index: Mutex::new(index),
        origin: origin.clone(),
        localhost_origin: format!("http://localhost:{port}"),
        secure,
        host,
        api,
        design_validation: tokio::sync::Semaphore::new(1),
    });
    let app = app::router(panel);
    println!("Listening on {bind_ip}:{port} · {origin}");
    topcoat::serve(listener, app).await?;
    Ok(())
}

fn redirect(url: &str) -> Response {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, url)
        .body(Body::empty())
        .expect("valid application redirect")
}

fn status(code: StatusCode) -> Response {
    Response::builder()
        .status(code)
        .body(Body::empty())
        .unwrap()
}

fn db_error(error: impl std::fmt::Display) -> StatusCode {
    eprintln!("panel: {error}");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
