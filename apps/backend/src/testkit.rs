//! Helpers for API tests: a real server on an in-memory database, spoken to over
//! raw HTTP/1.1 so tests exercise every layer, middleware included.

use crate::{app, db, security, send, state};
use serde::de::DeserializeOwned;
use std::net::{Ipv4Addr, SocketAddr};

pub fn settings() -> security::Security {
    security::Security {
        rate_burst: 60,
        rate_refill_ms: 100,
        body_limit: 1024 * 1024,
        blob_limit: 4 * 1024 * 1024,
        request_timeout: 30,
        max_concurrency: 16,
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
        .with_env_filter("rupicola_backend=debug")
        .try_init();
    let state = state::AppState::new(
        db::open_memory().await,
        crate::blobs::Blobs::memory(),
        setup_code,
        open_setup,
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = app(&settings, state.clone()).into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (port, state)
}

pub async fn request(port: u16, raw: String) -> String {
    tokio::task::spawn_blocking(move || send(port, &raw))
        .await
        .unwrap()
        .unwrap()
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

pub fn status(response: &str) -> u16 {
    response
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_default()
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
    .bind(crate::auth::hash(PASSWORD).unwrap())
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
    assert_eq!(status(&response), 201, "{response}");
    token_of(&response)
}
