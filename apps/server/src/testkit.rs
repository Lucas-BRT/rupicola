//! Helpers for API tests: a real server on an in-memory database, spoken to over
//! raw HTTP/1.1 so tests exercise every layer, middleware included.

use crate::{
    http::{app, security},
    repositories as db, state,
};
use axum::http::StatusCode;
use rupicola_api::{
    CaptureStatus,
    record::{
        Appearance, CaptureRecord, Display, Environment, Member, MemberRole, ParentRef,
        RECORD_FORMAT, RecordKind, ScenarioRef,
    },
};
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr};

/// Media uploads in tests are tiny; a small cap keeps mistakes visible.
const TEST_BLOB_LIMIT: usize = 4 * 1024 * 1024;
/// Enough parallel requests for any test, small enough to stay cheap.
const TEST_MAX_CONCURRENCY: usize = 16;
/// A refill slower than any test runs, so a spent rate budget stays spent.
pub const NO_REFILL_MS: u64 = 60_000;

/// Production defaults, except the limits above.
pub fn settings() -> security::Security {
    security::Security {
        rate_burst: security::DEFAULT_RATE_BURST,
        rate_refill_ms: security::DEFAULT_RATE_REFILL_MS,
        body_limit: security::DEFAULT_BODY_LIMIT,
        blob_limit: TEST_BLOB_LIMIT,
        request_timeout: security::DEFAULT_REQUEST_TIMEOUT_SECS,
        max_concurrency: TEST_MAX_CONCURRENCY,
        behind_proxy: false,
    }
}

/// Starts a server on a fresh in-memory database.
pub async fn start(settings: security::Security) -> u16 {
    start_custom(settings, None, true).await.0
}

pub async fn start_custom(
    settings: security::Security,
    setup_code: Option<&str>,
    open_setup: bool,
) -> (u16, state::Shared) {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("rupicola_server=debug")
        .try_init();
    let state = state::AppState::new(
        db::open_memory().await,
        crate::storage::blobs::Blobs::memory(),
        setup_code,
        open_setup,
        crate::config::UploadLimits::default()
            .to_defaults(settings.blob_limit, settings.body_limit),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = app(&settings, state.clone())
        .unwrap()
        .into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (port, state)
}

/// Sends `head` and then `body` as raw bytes, which may not be text, and reads the answer.
pub async fn request_bytes(port: u16, head: String, body: Vec<u8>) -> String {
    tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(RAW_READ_TIMEOUT_SECS)))?;
        stream.write_all(head.as_bytes())?;
        stream.write_all(&body)?;
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer)?;
        std::io::Result::Ok(String::from_utf8_lossy(&answer).into_owned())
    })
    .await
    .unwrap()
    .unwrap()
}

/// Uploads `data` to `project` through the streamed route; returns its digest.
pub async fn stream_blob(port: u16, token: &str, project: &str, data: &[u8]) -> String {
    use sha2::Digest as _;
    let sha = format!("{:x}", sha2::Sha256::digest(data));
    let response = request_bytes(
        port,
        format!(
            "PUT /v1/projects/{project}/blobs/{sha} HTTP/1.1\r\nHost: test\r\n\
             Authorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            data.len()
        ),
        data.to_vec(),
    )
    .await;
    assert!(
        matches!(status(&response), StatusCode::OK | StatusCode::CREATED),
        "{response}"
    );
    sha
}

/// Seconds past the server's own request timeout that a raw request keeps waiting, so a
/// slow handler is answered by the server's 408 rather than cut off by the client.
const RAW_READ_MARGIN_SECS: u64 = 5;
/// Seconds a raw request waits for its answer.
const RAW_READ_TIMEOUT_SECS: u64 = security::DEFAULT_REQUEST_TIMEOUT_SECS + RAW_READ_MARGIN_SECS;

pub async fn request(port: u16, raw: String) -> String {
    request_bytes(port, raw, Vec::new()).await
}

pub async fn get(port: u16, path: &str, extra_headers: &str) -> String {
    request(
        port,
        format!("GET {path} HTTP/1.1\r\nHost: test\r\n{extra_headers}Connection: close\r\n\r\n"),
    )
    .await
}

/// Sends a request with an optional bearer token and JSON body.
pub async fn call(
    port: u16,
    method: &str,
    path: &str,
    token: Option<&str>,
    json: Option<&str>,
) -> String {
    let auth = token.map_or(String::new(), |token| {
        format!("Authorization: Bearer {token}\r\n")
    });
    let body = json.unwrap_or("");
    request(
        port,
        format!(
            "{method} {path} HTTP/1.1\r\nHost: test\r\n{auth}Content-Type: application/json\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
    .await
}

/// Status of a raw HTTP response.
pub fn status(response: &str) -> StatusCode {
    crate::status_code(response).unwrap_or_else(|| panic!("not an HTTP response: {response}"))
}

pub fn body(response: &str) -> &str {
    response.split_once("\r\n\r\n").unwrap().1
}

pub fn json<T: DeserializeOwned>(response: &str) -> T {
    serde_json::from_str(body(response)).unwrap_or_else(|error| panic!("{error}: {response}"))
}

pub fn header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
    response.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

pub fn token_of(response: &str) -> String {
    json::<rupicola_api::SessionToken>(response).token
}

/// Creates an account directly in the database, as an invitation will.
pub async fn seed_account(state: &state::Shared, handle: &str, admin: bool) {
    sqlx::query(
        "INSERT INTO accounts (handle, password_hash, admin, created_at) VALUES (?1, ?2, ?3, 0)",
    )
    .bind(handle)
    .bind(crate::services::passwords::hash(PASSWORD).unwrap())
    .bind(admin)
    .execute(&state.db)
    .await
    .unwrap();
}

pub const PASSWORD: &str = "correct horse battery";

/// Signs `handle` in with [`PASSWORD`] and returns the bearer token.
pub async fn sign_in(port: u16, handle: &str) -> String {
    let form = format!(r#"{{"handle":"{handle}","password":"{PASSWORD}"}}"#);
    let response = call(port, "POST", "/v1/sessions", None, Some(&form)).await;
    assert_eq!(status(&response), StatusCode::CREATED, "{response}");
    token_of(&response)
}

/// Two projects: `demo` holds the artifact `shot`, `other` holds `foreign-shot`.
/// Ana (owner of `demo`), Bruno (reader of `demo`) and Carla (owner of `other`).
pub const USERS: usize = 3;
pub const PROJECT_ARTIFACT: &str = "shot";
pub const FOREIGN_ARTIFACT: &str = "foreign-shot";
const DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

async fn insert_capture(db: &sqlx::SqlitePool, project: &str, artifact: &str) {
    let capture = format!("capture-{artifact}");
    sqlx::query(
        "INSERT INTO captures (id, project_id, scenario, status, inspection, environment, created_at)
         VALUES (?1, ?2, 'home', 'complete', 'pending', '{}', 0)",
    )
    .bind(&capture)
    .bind(project)
    .execute(db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO artifacts (id, capture_id, role, owner_id, position, path, sha256, bytes, mime)
         VALUES (?1, ?2, 'checkpoint', 'home', 0, 'home.png', ?3, 1, 'image/png')",
    )
    .bind(artifact)
    .bind(&capture)
    .bind(DIGEST)
    .execute(db)
    .await
    .unwrap();
}

pub async fn demo_with_artifacts() -> (u16, [String; USERS]) {
    let (port, state) = start_custom(settings(), None, true).await;
    for handle in ["ana", "bruno", "carla"] {
        seed_account(&state, handle, false).await;
    }
    let [ana, bruno, carla] = [
        sign_in(port, "ana").await,
        sign_in(port, "bruno").await,
        sign_in(port, "carla").await,
    ];
    for (project, owner) in [("demo", &ana), ("other", &carla)] {
        call(
            port,
            "POST",
            "/v1/projects",
            Some(owner),
            Some(&format!(r#"{{"id":"{project}","label":"{project}"}}"#)),
        )
        .await;
    }
    call(
        port,
        "PUT",
        "/v1/projects/demo/members/bruno",
        Some(&ana),
        Some(r#"{"role":"reader"}"#),
    )
    .await;
    insert_capture(&state.db, "demo", PROJECT_ARTIFACT).await;
    insert_capture(&state.db, "other", FOREIGN_ARTIFACT).await;
    (port, [ana, bruno, carla])
}

/// A one-file capture record of project `demo`, citing the blob `sha` of `bytes` bytes.
pub fn capture_record(id: &str, sha: &str, bytes: u64, parents: Vec<ParentRef>) -> CaptureRecord {
    CaptureRecord {
        format: RECORD_FORMAT,
        id: id.to_owned(),
        project: "demo".to_owned(),
        kind: RecordKind::Scenario,
        scenario: ScenarioRef {
            name: "Home".to_owned(),
            sha256: None,
        },
        revision: None,
        series: "Home@web".to_owned(),
        parents,
        captured_at: 1_795_000_000_000,
        status: CaptureStatus::Complete,
        blocker: None,
        environment: Environment {
            display: Some(Display {
                width: Some(800),
                height: Some(600),
                scale_milli: Some(2000),
                orientation: None,
            }),
            appearance: Some(Appearance {
                color_scheme: Some("dark".to_owned()),
                ..Appearance::default()
            }),
            ..Environment::default()
        },
        members: vec![Member {
            path: "checkpoints/home.png".to_owned(),
            sha256: sha.to_owned(),
            bytes,
            mime: "image/png".to_owned(),
            role: MemberRole::Checkpoint,
            owner: "home".to_owned(),
            surface: Some("home".to_owned()),
            locator: BTreeMap::from([("route".to_owned(), "/".to_owned())]),
            target_ms: None,
            video_ms: None,
        }],
    }
}

/// Publishes `record` to project `demo`.
pub async fn publish_record(port: u16, token: &str, record: &CaptureRecord) -> String {
    call(
        port,
        "PUT",
        &format!("/v1/projects/demo/captures/{}", record.id),
        Some(token),
        Some(&serde_json::to_string(record).unwrap()),
    )
    .await
}
