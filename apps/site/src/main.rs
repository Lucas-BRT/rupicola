use std::{
    error::Error,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    sync::Arc,
    time::Duration,
};

use clap::Parser;
use http::{StatusCode, header};
use tokio::net::TcpListener;
use topcoat::{
    Result as TopcoatResult,
    context::{Cx, app_context},
    router::{
        Body, Router,
        content::Html,
        path_param,
        response::{IntoResponse, Response},
        route,
    },
};

/// Port the site listens on unless told otherwise.
const DEFAULT_PORT: u16 = 4732;
/// How long the health probe waits to connect and to read the answer.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

mod docs;
mod pages;

const SITE_CSS: &str = include_str!("site.css");
const SITE_JS: &str = include_str!("site.js");
const INSTALL_SH: &str = include_str!("install.sh");

path_param!(*slug);

#[derive(Parser)]
#[command(about = "Serve the Rupicola landing page and documentation")]
struct Args {
    /// Bind IPv4 address (defaults to HOST env var or 127.0.0.1).
    #[arg(long)]
    bind: Option<Ipv4Addr>,
    /// Listen port (defaults to PORT env var or 4732).
    #[arg(long)]
    port: Option<u16>,
    /// Public URL of the Rupicola review panel.
    #[arg(long)]
    panel_url: Option<String>,
    /// Run a self-contained health probe against the local listener and exit.
    #[arg(long)]
    healthcheck: bool,
}

#[derive(Clone)]
struct SiteConfig {
    panel_url: String,
}

fn cfg(cx: &Cx) -> Arc<SiteConfig> {
    app_context::<Arc<SiteConfig>>(cx).clone()
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

    let panel_url = args
        .panel_url
        .or_else(|| std::env::var("RUPICOLA_PANEL_URL").ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://rupicola.lucasbrt.dev/dashboard".to_string());

    let shared = Arc::new(SiteConfig { panel_url });

    let app = Router::builder()
        .app_context(shared)
        .route(home)
        .route(install)
        .route(architecture)
        .route(docs_index)
        .route(docs_slug)
        .route(css)
        .route(js)
        .route(install_script)
        .route(healthz)
        .build();

    let addr = SocketAddr::from((host, port));
    let listener = TcpListener::bind(addr).await?;
    println!("rupicola-site listening on http://{addr}");
    topcoat::serve(listener, app).await?;
    Ok(())
}

#[route(GET "/")]
async fn home(cx: &Cx) -> TopcoatResult<Response> {
    Html(pages::render_home(&cfg(cx).panel_url)).into_response(cx)
}

#[route(GET "/install")]
async fn install(cx: &Cx) -> TopcoatResult<Response> {
    Html(pages::render_install(&cfg(cx).panel_url)).into_response(cx)
}

#[route(GET "/architecture")]
async fn architecture(cx: &Cx) -> TopcoatResult<Response> {
    Html(pages::render_architecture(&cfg(cx).panel_url)).into_response(cx)
}

#[route(GET "/docs")]
async fn docs_index(cx: &Cx) -> TopcoatResult<Response> {
    Html(pages::render_docs(&docs::DOCS[0], &cfg(cx).panel_url)).into_response(cx)
}

#[route(GET "/docs/{*slug}")]
async fn docs_slug(cx: &Cx) -> TopcoatResult<Response> {
    let slug = path_param::<Slug>(cx).collect::<Vec<_>>().join("/");
    match docs::find_doc(&slug) {
        Some(doc) => Html(pages::render_docs(doc, &cfg(cx).panel_url)).into_response(cx),
        None => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from("Document not found"))
            .expect("valid 404 response")),
    }
}

#[route(GET "/site.css")]
async fn css(_cx: &Cx) -> TopcoatResult<Response> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(SITE_CSS))
        .expect("valid css response"))
}

#[route(GET "/site.js")]
async fn js(_cx: &Cx) -> TopcoatResult<Response> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(SITE_JS))
        .expect("valid js response"))
}

#[route(GET "/install.sh")]
async fn install_script(_cx: &Cx) -> TopcoatResult<Response> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=300")
        .body(Body::from(INSTALL_SH))
        .expect("valid install.sh response"))
}

#[route(GET "/healthz")]
async fn healthz(_cx: &Cx) -> TopcoatResult<Response> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("ok"))
        .expect("valid healthz response"))
}
