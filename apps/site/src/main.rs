use std::{
    error::Error,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};

use app::SiteConfig;
use clap::Parser;
use tokio::net::TcpListener;

/// Port the site listens on unless told otherwise.
const DEFAULT_PORT: u16 = 4732;
/// Path of the review panel under the public URL.
const DASHBOARD_PATH: &str = "/dashboard";
/// How long the health probe waits to connect and to read the answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

mod app;
mod components;
mod content;
mod views;

#[derive(Parser)]
#[command(about = "Serve the Rupicola landing page and documentation")]
struct Args {
    /// Bind IPv4 address (defaults to HOST env var or 127.0.0.1).
    #[arg(long)]
    bind: Option<Ipv4Addr>,
    /// Listen port (defaults to PORT env var or 4732).
    #[arg(long)]
    port: Option<u16>,
    /// Public URL of this site, used in install commands (defaults to `RUPICOLA_PUBLIC_URL`
    /// or the local listener).
    #[arg(long)]
    public_url: Option<String>,
    /// Public URL of the Rupicola review panel (defaults to `RUPICOLA_PANEL_URL` or
    /// `<public url>/dashboard`).
    #[arg(long)]
    panel_url: Option<String>,
    /// Run a self-contained health probe against the local listener and exit.
    #[arg(long)]
    healthcheck: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = Args::parse();
    let port = args
        .port
        .or_else(|| std::env::var("PORT").ok().and_then(|v| v.parse().ok()))
        .unwrap_or(DEFAULT_PORT);

    if args.healthcheck {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let mut stream = TcpStream::connect_timeout(&addr, PROBE_TIMEOUT)?;
        stream.set_read_timeout(Some(PROBE_TIMEOUT))?;
        write!(
            stream,
            "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        )?;
        let mut resp = String::new();
        stream.read_to_string(&mut resp)?;
        if !resp.starts_with("HTTP/1.1 200") {
            return Err("healthcheck returned non-200".into());
        }
        return Ok(());
    }

    let host = args
        .bind
        .or_else(|| std::env::var("HOST").ok().and_then(|v| v.parse().ok()))
        .unwrap_or(Ipv4Addr::LOCALHOST);

    let public_url = args
        .public_url
        .or_else(|| std::env::var("RUPICOLA_PUBLIC_URL").ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("http://{host}:{port}"));
    let public_url = public_url.trim_end_matches('/').to_owned();
    let panel_url = args
        .panel_url
        .or_else(|| std::env::var("RUPICOLA_PANEL_URL").ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("{public_url}{DASHBOARD_PATH}"));

    let app = app::router(SiteConfig {
        public_url,
        panel_url,
    });

    let addr = SocketAddr::from((host, port));
    let listener = TcpListener::bind(addr).await?;
    println!("rupicola-site listening on http://{addr}");
    topcoat::serve(listener, app).await?;
    Ok(())
}
