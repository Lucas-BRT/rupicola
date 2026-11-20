use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    net::Ipv4Addr,
    path::{Component, Path as FilePath, PathBuf},
    sync::Arc,
};

use clap::Parser;
use http::{StatusCode, header};
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
use tower_http::services::ServeFile;
use turso::{Connection, Value, transaction::TransactionBehavior};

macro_rules! shell {
    ($cx:ident, $page:expr, $account:ident, $panel:ident, $active:expr, { $($content:tt)* }) => {
        view! {
            $cx =>
            <!DOCTYPE html>
            <html lang="en">
                <head>
                    <meta charset="utf-8">
                    <meta name="viewport" content="width=device-width, initial-scale=1">
                    <title>($page) " · Rupicola"</title>
                    <link rel="stylesheet" href="/panel.css">
                    <script src="/panel.js" defer=""></script>
                </head>
                <body>
                    <a class="skip" href="#main">"Skip to content"</a>
                    <div class="workspace">
                        <header class="topbar">
                            <div class="topbar-inner">
                                <a class="brand" href="/">
                                    <span class="mark" aria-hidden="true">"R"</span>
                                    <strong>"Rupicola"</strong>
                                    <span class="workspace-tag">(if $panel.secure { "CLOUD" } else { "LOCAL" })</span>
                                </a>
                                <nav aria-label="Workspace">
                                    <a href="/" if $active == "dashboard" { aria-current="page" }>"Dashboard"</a>
                                    <a href="/projects" if $active == "projects" { aria-current="page" }>"Projects"</a>
                                    <a href="/settings" if $active == "settings" { aria-current="page" }>"Settings"</a>
                                </nav>
                                <div class="topbar-account">
                                    <a href="/settings">(&$account.handle)</a>
                                    <form method="post" action="/logout">
                                        <input type="hidden" name="csrf" value=(&$account.csrf)>
                                        <button class="link-button" type="submit">"Sign out"</button>
                                    </form>
                                </div>
                            </div>
                        </header>
                        <main id="main" tabindex="-1">$($content)*</main>
                    </div>
                </body>
            </html>
        }
    };
}

mod auth;
mod components;
mod ingest;
mod media;
mod pages;
mod pipeline;
mod proposes;
mod routes;
use auth::Account;
use rupicola::{design_record, fix_record, problem_record, proposal_record, storage};

type BoxError = Box<dyn Error + Send + Sync>;
type PageResult<T> = Result<T, StatusCode>;

#[derive(Parser)]
#[command(about = "Browse local Rupicola visual evidence")]
struct Args {
    #[command(subcommand)]
    action: Option<Action>,
    /// Capture directories to index. May be repeated.
    #[arg(long, default_value = ".rupicola/captures")]
    captures: Vec<PathBuf>,
    /// Local, rebuildable index file.
    #[arg(long, default_value = ".rupicola/panel.db")]
    db: PathBuf,
    /// Bind IPv4 address (defaults to HOST env var or 127.0.0.1).
    #[arg(long)]
    bind: Option<Ipv4Addr>,
    #[arg(long)]
    port: Option<u16>,
    /// Public HTTPS origin when running behind a reverse proxy (or RUPICOLA_PUBLIC_ORIGIN).
    #[arg(long)]
    public_origin: Option<String>,
    /// Managed directory for incoming captures (or RUPICOLA_UPLOADS). Required in cloud mode.
    #[arg(long)]
    uploads: Option<PathBuf>,
    /// Run a self-contained health probe against the local listener and exit.
    #[arg(long)]
    healthcheck: bool,
}

#[derive(clap::Subcommand)]
enum Action {
    /// Send a capture to the shared panel using RUPICOLA_PUBLISH_TOKEN.
    Publish {
        run: PathBuf,
        #[arg(long)]
        server: String,
    },
    /// Reset a password from the machine hosting the panel.
    Recover { username: String },
}

struct Panel {
    design: PathBuf,
    roots: Vec<PathBuf>,
    index: Mutex<Connection>,
    origin: String,
    localhost_origin: String,
    host: String,
    secure: bool,
    bootstrap: Option<String>,
    dummy_password: String,
    password_work: Arc<tokio::sync::Semaphore>,
    upload_work: Arc<tokio::sync::Semaphore>,
    login_attempts: Mutex<BTreeMap<String, (u32, i64)>>,
    uploads: Option<PathBuf>,
}

#[derive(Deserialize)]
struct Project {
    id: String,
    label: String,
}

#[derive(Deserialize)]
struct Manifest {
    version: u32,
    project: Project,
    scenario: String,
    revision: Option<String>,
    created_at_ms: i64,
    status: String,
    blocker: Option<String>,
    inspection: String,
    environment: Environment,
    checkpoints: Vec<Checkpoint>,
    clips: Vec<Clip>,
    #[serde(default)]
    findings: Vec<Finding>,
}

#[derive(Deserialize)]
struct Finding {
    note: String,
    evidence: Option<String>,
}

#[derive(Deserialize, PartialEq, Eq)]
struct Environment {
    #[serde(default)]
    color_scheme: String,
    #[serde(default)]
    reduced_motion: bool,
    width: Option<u32>,
    height: Option<u32>,
    renderer: String,
}

#[derive(Deserialize)]
struct Checkpoint {
    id: String,
    path: String,
}

#[derive(Deserialize)]
struct Clip {
    id: String,
    frames: Vec<Frame>,
    analysis_path: Option<String>,
    video_path: Option<String>,
    sheet_path: Option<String>,
}

#[derive(Deserialize)]
struct Frame {
    path: String,
    target_ms: f64,
    video_ms: Option<f64>,
}

struct Run {
    key: String,
    project_id: String,
    project: String,
    scenario: String,
    status: String,
    inspection: String,
    created_at_ms: i64,
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

    if let Some(Action::Publish { run, server }) = args.action.as_ref() {
        return ingest::send(run, server);
    }
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
    index
        .execute(
            "CREATE TABLE IF NOT EXISTS runs_v1 (
              key TEXT PRIMARY KEY, project_id TEXT NOT NULL, project TEXT NOT NULL,
              scenario TEXT NOT NULL, status TEXT NOT NULL, inspection TEXT NOT NULL,
              created_at_ms INTEGER NOT NULL
            )",
            (),
        )
        .await?;

    auth::init(&index).await?;
    if let Some(Action::Recover { username }) = args.action {
        return auth::recover(&index, &username).await;
    }
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
    let bootstrap = std::env::var("RUPICOLA_SETUP_CODE")
        .ok()
        .filter(|s| !s.is_empty());
    if bootstrap.as_ref().is_some_and(|secret| secret.len() < 32) {
        return Err("RUPICOLA_SETUP_CODE must contain at least 32 characters".into());
    }
    let owner_exists = auth::owner_exists(&index).await?;
    let raw_uploads = args.uploads.or_else(|| {
        std::env::var("RUPICOLA_UPLOADS")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
    });
    if secure && (raw_uploads.is_none() || (!owner_exists && bootstrap.is_none())) {
        return Err(
            "cloud mode needs --uploads (or RUPICOLA_UPLOADS) and RUPICOLA_SETUP_CODE for initial registration".into(),
        );
    }
    let uploads = raw_uploads
        .map(|path| {
            fs::create_dir_all(&path)?;
            fs::canonicalize(path)
        })
        .transpose()?;
    let mut roots = if let Some(env_cap) = std::env::var("RUPICOLA_CAPTURES")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        let p = PathBuf::from(env_cap);
        let _ = fs::create_dir_all(&p);
        vec![p]
    } else {
        args.captures
    };
    if let Some(path) = &uploads {
        if roots
            .iter()
            .any(|root| fs::canonicalize(root).is_ok_and(|root| root == *path))
        {
            return Err("use a dedicated --uploads directory separate from --captures".into());
        }
        roots.push(path.clone());
    }
    let panel = Arc::new(Panel {
        design: db_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join(design_record::DIRECTORY),
        roots,
        index: Mutex::new(index),
        origin: origin.clone(),
        localhost_origin: format!("http://localhost:{port}"),
        secure,
        host,
        bootstrap: bootstrap.as_deref().map(auth::digest),
        dummy_password: auth::password_hash(&auth::token())?,
        password_work: Arc::new(tokio::sync::Semaphore::new(4)),
        upload_work: Arc::new(tokio::sync::Semaphore::new(2)),
        login_attempts: Mutex::new(BTreeMap::new()),
        uploads,
    });
    let app = routes::router(panel);
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

fn run_dir(panel: &Panel, root: usize, name: &str) -> Option<PathBuf> {
    if !safe_name(name) {
        return None;
    }
    let dir = panel.roots.get(root)?.join(name);
    fs::symlink_metadata(&dir).ok()?.is_dir().then_some(dir)
}

fn read_manifest(dir: &FilePath) -> Option<Manifest> {
    let path = dir.join("manifest.json");
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return None;
    }
    let run: Manifest = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (run.version == 1).then_some(run)
}

async fn read_index(panel: &Panel) -> Result<Vec<Run>, BoxError> {
    let index = panel.index.lock().await;
    let mut changes = vec![("DELETE FROM runs_v1", Vec::<Value>::new())];
    for (root_index, root) in panel.roots.iter().enumerate() {
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(dir) = name
                .to_str()
                .and_then(|name| run_dir(panel, root_index, name))
            else {
                continue;
            };
            let Some(run) = read_manifest(&dir) else {
                continue;
            };
            changes.push((
                "INSERT INTO runs_v1 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                vec![
                    Value::Text(format!("{root_index}/{}", name.to_string_lossy())),
                    Value::Text(run.project.id),
                    Value::Text(run.project.label),
                    Value::Text(run.scenario),
                    Value::Text(run.status),
                    Value::Text(run.inspection),
                    Value::Integer(run.created_at_ms),
                ],
            ));
        }
    }
    index
        .transactional_batch(changes, TransactionBehavior::Immediate)
        .await?;
    let mut rows = index.query(
        "SELECT key, project_id, project, scenario, status, inspection, created_at_ms FROM runs_v1
         ORDER BY created_at_ms DESC, key",
        (),
    ).await?;
    let mut runs = Vec::new();
    while let Some(row) = rows.next().await? {
        runs.push(Run {
            key: row.get(0)?,
            project_id: row.get(1)?,
            project: row.get(2)?,
            scenario: row.get(3)?,
            status: row.get(4)?,
            inspection: row.get(5)?,
            created_at_ms: row.get(6)?,
        });
    }
    Ok(runs)
}

#[derive(Deserialize)]
struct Comparison {
    before: Option<String>,
}

async fn detail(
    comparison: Comparison,
    account: Account,
    panel: Arc<Panel>,
    (root, name): (usize, String),
) -> PageResult<Html<String>> {
    let run = run_dir(&panel, root, &name)
        .and_then(|dir| read_manifest(&dir))
        .ok_or(StatusCode::NOT_FOUND)?;
    let role = auth::role(&panel, &account, &run.project.id).await?;
    let key = format!("{root}/{name}");
    let url = |path: &str| format!("/media/{root}/{name}/{path}");
    let candidates: Vec<_> = read_index(&panel)
        .await
        .map_err(db_error)?
        .into_iter()
        .filter(|entry| {
            entry.project_id == run.project.id
                && entry.scenario == run.scenario
                && entry.status == "complete"
                && entry.key != key
        })
        .collect();
    let requested = comparison.before;
    let before = requested.as_deref().and_then(|key| {
        let (root, name) = key.split_once('/')?;
        let manifest = read_manifest(&run_dir(&panel, root.parse().ok()?, name)?)?;
        (manifest.project.id == run.project.id
            && manifest.scenario == run.scenario
            && manifest.status == "complete")
            .then_some((
                key.to_owned(),
                manifest.checkpoints,
                manifest.environment != run.environment,
            ))
    });
    let environments_differ = before.as_ref().is_some_and(|(_, _, differs)| *differs);
    let before_images: BTreeMap<_, _> = before
        .as_ref()
        .into_iter()
        .flat_map(|(before_key, checkpoints, _)| {
            checkpoints.iter().map(move |item| {
                (
                    item.id.clone(),
                    format!("/media/{before_key}/{}", item.path),
                )
            })
        })
        .collect();
    let width = run.environment.width.unwrap_or(640).max(1).to_string();
    let height = run.environment.height.unwrap_or(480).max(1).to_string();
    let cx = &Cx::default();
    let may_review = role != auth::Role::Reader;
    let comparing = before.is_some();
    let project_url = format!("/projects/{}", run.project.id);
    let form_url = format!("/run/{key}");
    let content = shell!(cx, &run.scenario, account, panel, "projects", {
                    <nav class="breadcrumbs" aria-label="Breadcrumb">
                      <a href="/projects">"Projects"</a><span aria-hidden="true">"/"</span>
                      <a href=(&project_url)>(&run.project.label)</a>
                    </nav>
                    <header class="page-title project-masthead">
                        <h1>(&run.scenario)</h1>
                        <a class="button" href=(format!("{project_url}/captures"))>"Project captures"</a>
                    </header>
                    <p class="metadata run-metadata"><span class="status">(&run.status)</span> <span>"Inspection " (&run.inspection)</span> <code>(&run.environment.renderer)</code></p>
                    <p class="muted">
                        (format!("{} × {}", width, height))
                        if !run.environment.color_scheme.is_empty() { " · " (&run.environment.color_scheme) }
                        if run.environment.reduced_motion { " · Reduced motion" }
                    </p>
                    if let Some(revision) = &run.revision { <p>"Revision " <code>(revision)</code></p> }
                    if let Some(blocker) = &run.blocker { <p class="blocker">(blocker)</p> }
                    if !candidates.is_empty() {
                        <form class="filters comparison-control" method="get" action=(&form_url)>
                            <label for="before">"Compare with"</label>
                            <select id="before" name="before">
                                <option value="">"Current run only"</option>
                                for candidate in &candidates {
                                    <option value=(&candidate.key) data-epoch-ms=(candidate.created_at_ms.to_string())
                                        data-time-suffix=(candidate.key.split_once('/').map(|(_, name)| name.chars().take(16).collect::<String>()).unwrap_or_default())
                                        if requested.as_ref() == Some(&candidate.key) { selected="" }>
                                        (&candidate.key)
                                    </option>
                                }
                            </select>
                            <button class="button" type="submit">"Apply"</button>
                        </form>
                    }
                    if environments_differ {
                        <p class="notice">"The two runs used different capture settings. Check their environment before judging differences."</p>
                    }
                    if !run.findings.is_empty() {
                        <section>
                            <h2>"Findings"</h2>
                            <ul>
                                for finding in &run.findings {
                                    <li>
                                        (&finding.note)
                                        if let Some(evidence) = &finding.evidence {
                                            " " <a href=(url(evidence))>"Evidence"</a>
                                        }
                                    </li>
                                }
                            </ul>
                        </section>
                    }
                    if !run.checkpoints.is_empty() {
                        <section>
                            <h2>"Screenshots"</h2>
                            <div class=(if comparing { "compare" } else { "frames" })>
                                for item in &run.checkpoints {
                                    <div class=(if before_images.contains_key(item.id.as_str()) { "checkpoint pair" } else { "checkpoint" })>
                                    if let Some(previous) = before_images.get(item.id.as_str()) {
                                        <figure>
                                            <a href=(previous)>
                                                <img src=(previous) alt=(format!("Previous run: {}", item.id)) loading="lazy">
                                            </a>
                                            <figcaption>"Previous run · " (&item.id)</figcaption>
                                        </figure>
                                    }
                                    <figure>
                                        <a href=(url(&item.path))>
                                            <img src=(url(&item.path)) alt=(format!("Screenshot: {}", item.id)) loading="lazy">
                                        </a>
                                        <figcaption>
                                            if before_images.contains_key(item.id.as_str()) { "Current run · " }
                                            (&item.id)
                                        </figcaption>
                                    </figure>
                                    </div>
                                }
                            </div>
                        </section>
                    }
                    if !run.clips.is_empty() {
                        <section>
                            <h2>"Recordings"</h2>
                            for clip in &run.clips {
                                <article>
                                    <h3>(&clip.id)</h3>
                                    if let Some(video) = &clip.video_path {
                                        <video controls="" playsinline="" preload="metadata" loading="lazy" src=(url(video))
                                            width=(&width) height=(&height) aria-label=(&clip.id)
                                            if let Some(first) = clip.frames.first() { poster=(url(&first.path)) }></video>
                                    }
                                    if !clip.frames.is_empty() {
                                        <div class="frames">
                                            for frame in &clip.frames {
                                                <figure>
                                                    <a href=(url(&frame.path))>
                                                        <img src=(url(&frame.path)) alt=(format!("{} at {:.0} ms", clip.id, frame.target_ms)) loading="lazy">
                                                    </a>
                                                    <figcaption>
                                                        if clip.video_path.is_some() {
                                                            <button type="button" data-seek=(format!("{}", frame.video_ms.unwrap_or(frame.target_ms) / 1000.0))>
                                                                (format!("{:.0} ms", frame.video_ms.unwrap_or(frame.target_ms)))
                                                            </button>
                                                        } else {
                                                            (format!("{:.0} ms", frame.target_ms))
                                                        }
                                                    </figcaption>
                                                </figure>
                                            }
                                        </div>
                                    }
                                    <p class="downloads">
                                        if let Some(path) = &clip.sheet_path { <a href=(url(path))>"Contact sheet"</a> }
                                        if let Some(path) = &clip.analysis_path { <a href=(url(path))>"Frame analysis"</a> }
                                    </p>
                                </article>
                            }
                        </section>
                    }
                    if may_review {
                        <section class="surface review" id="review">
                            <h2>"Record inspection"</h2>
                            <form method="post" action=(&form_url) class="stack-form">
                                <input type="hidden" name="csrf" value=(&account.csrf)>
                                <label for="note">"Observation"</label>
                                <textarea id="note" name="note" required="" maxlength="2000" rows="3"></textarea>
                                if !run.checkpoints.is_empty() {
                                    <label for="evidence">"Evidence"</label>
                                    <select id="evidence" name="evidence">
                                        <option value="">"No checkpoint"</option>
                                        for checkpoint in &run.checkpoints {
                                            <option value=(&checkpoint.path)>(&checkpoint.id)</option>
                                        }
                                    </select>
                                }
                                <button type="submit" class="button primary">"Save inspection"</button>
                            </form>
                        </section>
                    }
    });
    auth::page(cx, content).await
}
