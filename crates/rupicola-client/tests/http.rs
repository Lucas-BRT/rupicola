//! `HttpApi` against a stub server: paths, bearer tokens and how statuses become errors.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::{delete, get, post, put},
};
use rupicola_api::{
    AuditEvent, Capture, CaptureStatus, Credentials, Environment, ErrorBody, Inspection, Invite,
    IssuedInvite, Me, Member, NewCapture, NewInvite, NewProject, PasswordChange, Project, Redeem,
    Role, SessionToken, SetupStatus, TokenKind,
};
use rupicola_client::{Api, ClientError, HttpApi, McpReply};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const TOKEN: &str = "stub-token";
const HANDLE: &str = "ada";
const PASSWORD: &str = "correct horse battery";
const RETRY_AFTER_SECS: u64 = 7;
const PROJECT: &str = "demo";
const LABEL: &str = "Demo project";
const CONFLICT_MESSAGE: &str = "a project with this id already exists";
const KNOWN_BLOB: &[u8] = b"already stored";
const NEW_BLOB: &[u8] = b"not stored yet";
const SCENARIO: &str = "Fixture";
const CAPTURE_ID: &str = "capture-1";
const MCP_PAYLOAD: &str = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
const MCP_BODY: &str = "pong";

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<ErrorBody>) {
    (
        status,
        Json(ErrorBody {
            code: code.to_owned(),
            message: message.to_owned(),
        }),
    )
}

fn project() -> Project {
    Project {
        id: PROJECT.to_owned(),
        label: LABEL.to_owned(),
        created_at: 0,
        role: Some(Role::Owner),
    }
}

fn new_project() -> NewProject {
    NewProject {
        id: PROJECT.to_owned(),
        label: LABEL.to_owned(),
    }
}

fn credentials(password: &str) -> Credentials {
    Credentials {
        handle: HANDLE.to_owned(),
        password: password.to_owned(),
    }
}

fn sha256(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn new_capture() -> NewCapture {
    NewCapture {
        scenario: SCENARIO.to_owned(),
        revision: None,
        status: CaptureStatus::Blocked,
        blocker: None,
        environment: Environment::default(),
        checkpoints: vec![],
        clips: vec![],
    }
}

const INVITE_CODE: &str = "invite-code";
const AUDIT_EVENT: &str = "member-added";
const WRONG_PASSWORD_MESSAGE: &str = "the current password is incorrect";

/// Stub of the member, audit and invitation routes; each answers with what it was asked
/// for, so a test can tell the path reached it.
fn access_routes() -> Router {
    Router::new()
        .route(
            "/v1/projects/{id}/members",
            get(|Path(id): Path<String>| async move {
                Json(vec![Member {
                    handle: id,
                    role: Role::Reader,
                }])
            }),
        )
        .route(
            "/v1/projects/{id}/members/{handle}",
            delete(|Path((_, handle)): Path<(String, String)>| async move {
                if handle == HANDLE {
                    StatusCode::NO_CONTENT
                } else {
                    StatusCode::NOT_FOUND
                }
            }),
        )
        .route(
            "/v1/projects/{id}/audit",
            get(|Path(id): Path<String>| async move {
                Json(vec![AuditEvent {
                    id: "1".to_owned(),
                    actor: id,
                    event: AUDIT_EVENT.to_owned(),
                    occurred_at: 0,
                }])
            }),
        )
        .route(
            "/v1/projects/{id}/invites",
            post(|Json(form): Json<NewInvite>| async move {
                Json(IssuedInvite {
                    code: INVITE_CODE.to_owned(),
                    invite: Invite {
                        id: "digest".to_owned(),
                        role: form.role,
                        created_by: None,
                        expires_at: 0,
                    },
                })
            }),
        )
        .route(
            "/v1/me/password",
            put(|Json(form): Json<PasswordChange>| async move {
                if form.current == PASSWORD {
                    Ok(StatusCode::NO_CONTENT)
                } else {
                    Err(error(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "invalid",
                        WRONG_PASSWORD_MESSAGE,
                    ))
                }
            }),
        )
        .route(
            "/v1/sessions/others",
            delete(|| async { StatusCode::NO_CONTENT }),
        )
        .route(
            "/v1/invites/redeem",
            post(|Json(form): Json<Redeem>| async move {
                if form.code == INVITE_CODE {
                    Ok(Json(project()))
                } else {
                    Err(error(StatusCode::FORBIDDEN, "forbidden", "unknown code"))
                }
            }),
        )
}

/// Stub of the blob, capture and MCP routes; counts the uploads it receives.
fn upload_routes(uploads: Arc<AtomicUsize>) -> Router {
    Router::new()
        .route(
            "/v1/blobs/{sha256}",
            put(|State(uploads): State<Arc<AtomicUsize>>| async move {
                uploads.fetch_add(1, Ordering::SeqCst);
                StatusCode::CREATED
            })
            .head(|Path(sha256): Path<String>| async move {
                if sha256 == self::sha256(KNOWN_BLOB) {
                    StatusCode::OK
                } else {
                    StatusCode::NOT_FOUND
                }
            }),
        )
        .route(
            "/v1/projects/{id}/captures",
            post(
                |Path(id): Path<String>, Json(form): Json<NewCapture>| async move {
                    Json(Capture {
                        id: CAPTURE_ID.to_owned(),
                        project_id: id,
                        scenario: form.scenario,
                        revision: None,
                        status: form.status,
                        blocker: None,
                        inspection: Inspection::Pending,
                        environment: Environment::default(),
                        created_at: 0,
                        checkpoints: vec![],
                        clips: vec![],
                        findings: vec![],
                    })
                },
            ),
        )
        .route(
            "/mcp",
            post(|| async { (StatusCode::BAD_GATEWAY, MCP_BODY) }),
        )
        .with_state(uploads)
}

async fn serve() -> (HttpApi, Arc<AtomicUsize>) {
    let uploads = Arc::new(AtomicUsize::new(0));
    let app = upload_routes(Arc::clone(&uploads))
        .merge(access_routes())
        .route(
            "/v1/sessions",
            post(|Json(form): Json<Credentials>| async move {
                if form.password == PASSWORD {
                    Ok(Json(SessionToken {
                        token: TOKEN.to_owned(),
                        expires_at: 0,
                    }))
                } else {
                    Err((
                        StatusCode::TOO_MANY_REQUESTS,
                        [(header::RETRY_AFTER, RETRY_AFTER_SECS.to_string())],
                    ))
                }
            }),
        )
        .route(
            "/v1/setup",
            get(|| async { Json(SetupStatus { required: true }) }),
        )
        .route(
            "/v1/me",
            get(|headers: HeaderMap| async move {
                if bearer(&headers) == Some(TOKEN) {
                    Ok(Json(Me {
                        kind: TokenKind::Session,
                        handle: Some(HANDLE.to_owned()),
                        admin: false,
                        project: None,
                    }))
                } else {
                    Err(error(StatusCode::UNAUTHORIZED, "unauthorized", "sign in"))
                }
            }),
        )
        .route(
            "/v1/projects",
            get(|| async { Json(vec![project()]) }).post(|Json(_): Json<NewProject>| async {
                error(StatusCode::CONFLICT, "conflict", CONFLICT_MESSAGE)
            }),
        )
        .route(
            "/v1/sessions/current",
            delete(|| async { StatusCode::NO_CONTENT }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (
        HttpApi::new(&format!("http://{address}/")).unwrap(),
        uploads,
    )
}

#[tokio::test]
async fn calls_reach_their_routes_with_the_bearer_token() {
    let (api, _) = serve().await;
    let session = api.sign_in(&credentials(PASSWORD)).await.unwrap();
    assert_eq!(session.token, TOKEN);
    assert_eq!(api.me(TOKEN).await.unwrap().handle.as_deref(), Some(HANDLE));
    assert_eq!(api.projects(TOKEN).await.unwrap(), vec![project()]);
    assert!(api.setup_required().await.unwrap());
    api.sign_out(TOKEN).await.unwrap();
}

#[tokio::test]
async fn statuses_become_typed_errors() {
    let (api, _) = serve().await;
    assert_eq!(api.me("wrong").await.err(), Some(ClientError::Unauthorized));
    assert_eq!(
        api.sign_in(&credentials("")).await.err(),
        Some(ClientError::RateLimited {
            retry_after: Some(RETRY_AFTER_SECS)
        })
    );
    assert_eq!(
        api.create_project(TOKEN, &new_project()).await.err(),
        Some(ClientError::Conflict(CONFLICT_MESSAGE.to_owned()))
    );
}

#[tokio::test]
async fn an_unreachable_backend_is_reported_as_unavailable() {
    let api = HttpApi::new("http://127.0.0.1:1").unwrap();
    assert!(matches!(
        api.me(TOKEN).await,
        Err(ClientError::Unavailable(_))
    ));
}

#[tokio::test]
async fn a_blob_the_backend_has_is_not_uploaded_again() {
    let (api, uploads) = serve().await;
    assert_eq!(
        api.put_blob(TOKEN, KNOWN_BLOB).await.unwrap(),
        sha256(KNOWN_BLOB)
    );
    assert_eq!(uploads.load(Ordering::SeqCst), 0);
    assert_eq!(
        api.put_blob(TOKEN, NEW_BLOB).await.unwrap(),
        sha256(NEW_BLOB)
    );
    assert_eq!(uploads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_capture_is_published_to_its_project() {
    let (api, _) = serve().await;
    let capture = api
        .publish_capture(TOKEN, PROJECT, &new_capture())
        .await
        .unwrap();
    assert_eq!(capture.id, CAPTURE_ID);
    assert_eq!(capture.project_id, PROJECT);
    assert_eq!(capture.scenario, SCENARIO);
}

#[tokio::test]
async fn an_mcp_answer_is_relayed_whatever_its_status() {
    let (api, _) = serve().await;
    assert_eq!(
        api.mcp(TOKEN, MCP_PAYLOAD).await.unwrap(),
        McpReply {
            status: StatusCode::BAD_GATEWAY.as_u16(),
            body: MCP_BODY.to_owned()
        }
    );
}

#[tokio::test]
async fn project_access_is_read_and_changed_through_its_routes() {
    let (api, _) = serve().await;
    let members = api.members(TOKEN, PROJECT).await.unwrap();
    assert_eq!(members[0].handle, PROJECT);
    assert_eq!(
        api.audit(TOKEN, PROJECT).await.unwrap()[0].event,
        AUDIT_EVENT
    );
    api.remove_member(TOKEN, PROJECT, HANDLE).await.unwrap();
    assert_eq!(
        api.remove_member(TOKEN, PROJECT, "nobody").await.err(),
        Some(ClientError::NotFound)
    );
}

#[tokio::test]
async fn an_invitation_is_issued_and_redeemed() {
    let (api, _) = serve().await;
    let issued = api
        .create_invite(
            TOKEN,
            PROJECT,
            &NewInvite {
                role: Role::Reviewer,
                hours: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(issued.code, INVITE_CODE);
    assert_eq!(issued.invite.role, Role::Reviewer);
    let joined = api
        .redeem_invite(
            TOKEN,
            &Redeem {
                code: issued.code.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(joined, project());
    assert_eq!(
        api.redeem_invite(
            TOKEN,
            &Redeem {
                code: "wrong".to_owned()
            }
        )
        .await
        .err(),
        Some(ClientError::Forbidden)
    );
}

#[tokio::test]
async fn the_password_is_changed_and_other_sessions_can_be_ended() {
    let (api, _) = serve().await;
    let change = |current: &str| PasswordChange {
        current: current.to_owned(),
        new: "another long password".to_owned(),
    };
    api.change_password(TOKEN, &change(PASSWORD)).await.unwrap();
    assert_eq!(
        api.change_password(TOKEN, &change("wrong")).await.err(),
        Some(ClientError::Invalid(WRONG_PASSWORD_MESSAGE.to_owned()))
    );
    api.sign_out_others(TOKEN).await.unwrap();
}
