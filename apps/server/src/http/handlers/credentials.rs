//! `/tokens`, `/projects/{id}/devices`, `/projects/{id}/invites`, `/register` and
//! `/invites/redeem`.

use crate::{
    domain::{ApiError, Principal},
    services::credentials,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{
    Credential, Invite, IssuedCredential, IssuedInvite, NewCredential, NewInvite, Project, Redeem,
    Register, SessionToken,
};

/// `POST /v1/tokens`
pub async fn create_token(
    State(state): State<Shared>,
    principal: Principal,
    Json(form): Json<NewCredential>,
) -> Result<(StatusCode, Json<IssuedCredential>), ApiError> {
    let issued = credentials::create_token(&state, &principal, &form.label).await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

/// `GET /v1/tokens`
pub async fn list_tokens(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<Json<Vec<Credential>>, ApiError> {
    credentials::list_tokens(&state, &principal).await.map(Json)
}

/// `DELETE /v1/tokens/{id}`
pub async fn delete_token(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    credentials::delete_token(&state, &principal, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/projects/{id}/devices`
pub async fn create_device(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewCredential>,
) -> Result<(StatusCode, Json<IssuedCredential>), ApiError> {
    let issued = credentials::create_device(&state, &principal, &project, &form.label).await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

/// `GET /v1/projects/{id}/devices`
pub async fn list_devices(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<Credential>>, ApiError> {
    credentials::list_devices(&state, &principal, &project)
        .await
        .map(Json)
}

/// `DELETE /v1/projects/{id}/devices/{device}`
pub async fn delete_device(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, device)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    credentials::delete_device(&state, &principal, &project, &device).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/projects/{id}/invites`
pub async fn create_invite(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewInvite>,
) -> Result<(StatusCode, Json<IssuedInvite>), ApiError> {
    let issued = credentials::create_invite(&state, &principal, &project, form).await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

/// `GET /v1/projects/{id}/invites`
pub async fn list_invites(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<Invite>>, ApiError> {
    credentials::list_invites(&state, &principal, &project)
        .await
        .map(Json)
}

/// `DELETE /v1/projects/{id}/invites/{invite}`
pub async fn delete_invite(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, invite)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    credentials::delete_invite(&state, &principal, &project, &invite).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/register`
pub async fn register(
    State(state): State<Shared>,
    Json(form): Json<Register>,
) -> Result<(StatusCode, Json<SessionToken>), ApiError> {
    let session = credentials::register(&state, form).await?;
    Ok((StatusCode::CREATED, Json(session)))
}

/// `POST /v1/invites/redeem`
pub async fn redeem(
    State(state): State<Shared>,
    principal: Principal,
    Json(form): Json<Redeem>,
) -> Result<Json<Project>, ApiError> {
    credentials::redeem(&state, &principal, &form.code)
        .await
        .map(Json)
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{
        Credential, IssuedCredential, IssuedInvite, Me, Member, Project, Role, TokenKind,
    };

    /// Ana owns `demo`. Returns the port, the state and Ana's session.
    async fn demo() -> (u16, crate::state::Shared, String) {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        let ana = sign_in(port, "ana").await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        (port, state, ana)
    }

    async fn invite(port: u16, owner: &str, role: &str) -> String {
        let issued = call(
            port,
            "POST",
            "/v1/projects/demo/invites",
            Some(owner),
            Some(&format!(r#"{{"role":"{role}"}}"#)),
        )
        .await;
        assert_eq!(status(&issued), StatusCode::CREATED, "{issued}");
        json::<IssuedInvite>(&issued).code
    }

    fn registration(code: &str, handle: &str) -> String {
        format!(r#"{{"code":"{code}","handle":"{handle}","password":"{PASSWORD}"}}"#)
    }

    #[tokio::test]
    async fn an_invitation_registers_one_account_into_the_project() {
        let (port, _state, ana) = demo().await;
        let code = invite(port, &ana, "reviewer").await;

        let taken = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&code, "ana")),
        )
        .await;
        assert_eq!(status(&taken), StatusCode::CONFLICT, "{taken}");
        // A taken username does not burn the code.
        let joined = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&code, "bruno")),
        )
        .await;
        assert_eq!(status(&joined), StatusCode::CREATED, "{joined}");
        let bruno = token_of(&joined);
        let project: Project =
            json(&call(port, "GET", "/v1/projects/demo", Some(&bruno), None).await);
        assert_eq!(project.role, Some(Role::Reviewer));

        let reused = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&code, "carla")),
        )
        .await;
        assert_eq!(status(&reused), StatusCode::FORBIDDEN, "{reused}");
    }

    #[tokio::test]
    async fn expired_or_withdrawn_invitations_do_not_work() {
        let (port, state, ana) = demo().await;
        let expired = invite(port, &ana, "reader").await;
        sqlx::query("UPDATE invites SET expires_at = 0")
            .execute(&state.db)
            .await
            .unwrap();
        let late = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&expired, "bruno")),
        )
        .await;
        assert_eq!(status(&late), StatusCode::FORBIDDEN, "{late}");

        let withdrawn = invite(port, &ana, "reader").await;
        let pending: Vec<rupicola_api::Invite> =
            json(&call(port, "GET", "/v1/projects/demo/invites", Some(&ana), None).await);
        assert_eq!(pending.len(), 1);
        let gone = call(
            port,
            "DELETE",
            &format!("/v1/projects/demo/invites/{}", pending[0].id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&gone), StatusCode::NO_CONTENT, "{gone}");
        let refused = call(
            port,
            "POST",
            "/v1/register",
            None,
            Some(&registration(&withdrawn, "bruno")),
        )
        .await;
        assert_eq!(status(&refused), StatusCode::FORBIDDEN, "{refused}");
    }

    #[tokio::test]
    async fn redeeming_keeps_the_higher_role() {
        let (port, state, ana) = demo().await;
        seed_account(&state, "bruno", false).await;
        let bruno = sign_in(port, "bruno").await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reviewer"}"#),
        )
        .await;
        let code = invite(port, &ana, "reader").await;

        let joined = call(
            port,
            "POST",
            "/v1/invites/redeem",
            Some(&bruno),
            Some(&format!(r#"{{"code":"{code}"}}"#)),
        )
        .await;
        assert_eq!(status(&joined), StatusCode::OK, "{joined}");
        let members: Vec<Member> =
            json(&call(port, "GET", "/v1/projects/demo/members", Some(&ana), None).await);
        assert!(members.contains(&Member {
            handle: "bruno".into(),
            role: Role::Reviewer
        }));
    }

    #[tokio::test]
    async fn machine_tokens_act_for_their_owner_until_revoked() {
        let (port, _state, ana) = demo().await;
        let issued: IssuedCredential = json(
            &call(
                port,
                "POST",
                "/v1/tokens",
                Some(&ana),
                Some(r#"{"label":"Laptop agent"}"#),
            )
            .await,
        );
        let machine = issued.token;

        let me: Me = json(&call(port, "GET", "/v1/me", Some(&machine), None).await);
        assert_eq!(
            (me.kind, me.handle.as_deref()),
            (TokenKind::Machine, Some("ana"))
        );
        let project: Project =
            json(&call(port, "GET", "/v1/projects/demo", Some(&machine), None).await);
        assert_eq!(project.role, Some(Role::Owner));
        // An agent cannot mint more credentials.
        let minted = call(
            port,
            "POST",
            "/v1/tokens",
            Some(&machine),
            Some(r#"{"label":"Another"}"#),
        )
        .await;
        assert_eq!(status(&minted), StatusCode::FORBIDDEN, "{minted}");

        let listed: Vec<Credential> =
            json(&call(port, "GET", "/v1/tokens", Some(&ana), None).await);
        assert!(
            listed
                .iter()
                .any(|item| item.id == issued.credential.id && item.label == "Laptop agent")
        );
        let revoked = call(
            port,
            "DELETE",
            &format!("/v1/tokens/{}", issued.credential.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&revoked), StatusCode::NO_CONTENT, "{revoked}");
        let after = call(port, "GET", "/v1/me", Some(&machine), None).await;
        assert_eq!(status(&after), StatusCode::UNAUTHORIZED, "{after}");
    }

    #[tokio::test]
    async fn a_device_only_publishes_to_its_own_project() {
        let (port, _state, ana) = demo().await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"other","label":"Other"}"#),
        )
        .await;
        let issued: IssuedCredential = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/devices",
                Some(&ana),
                Some(r#"{"label":"CI runner"}"#),
            )
            .await,
        );
        let device = issued.token;

        let me: Me = json(&call(port, "GET", "/v1/me", Some(&device), None).await);
        assert_eq!(
            (me.kind, me.project.as_deref()),
            (TokenKind::Device, Some("demo"))
        );
        let blocked = r#"{"scenario":"ci","status":"blocked","blocker":"server down"}"#;
        let published = call(
            port,
            "POST",
            "/v1/projects/demo/captures",
            Some(&device),
            Some(blocked),
        )
        .await;
        assert_eq!(status(&published), StatusCode::CREATED, "{published}");
        let read = call(
            port,
            "GET",
            "/v1/projects/demo/captures",
            Some(&device),
            None,
        )
        .await;
        assert_eq!(status(&read), StatusCode::OK, "{read}");
        let owner_only = call(
            port,
            "GET",
            "/v1/projects/demo/devices",
            Some(&device),
            None,
        )
        .await;
        assert_eq!(status(&owner_only), StatusCode::FORBIDDEN, "{owner_only}");
        let elsewhere = call(
            port,
            "POST",
            "/v1/projects/other/captures",
            Some(&device),
            Some(blocked),
        )
        .await;
        assert_eq!(status(&elsewhere), StatusCode::NOT_FOUND, "{elsewhere}");

        let revoked = call(
            port,
            "DELETE",
            &format!("/v1/projects/demo/devices/{}", issued.credential.id),
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&revoked), StatusCode::NO_CONTENT, "{revoked}");
        let after = call(port, "GET", "/v1/me", Some(&device), None).await;
        assert_eq!(status(&after), StatusCode::UNAUTHORIZED, "{after}");
    }

    #[tokio::test]
    async fn only_owners_issue_project_credentials() {
        let (port, state, ana) = demo().await;
        seed_account(&state, "bruno", false).await;
        let bruno = sign_in(port, "bruno").await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reviewer"}"#),
        )
        .await;
        for (path, body) in [
            ("/v1/projects/demo/invites", r#"{"role":"owner"}"#),
            ("/v1/projects/demo/devices", r#"{"label":"Sneaky"}"#),
        ] {
            let response = call(port, "POST", path, Some(&bruno), Some(body)).await;
            assert_eq!(
                status(&response),
                StatusCode::FORBIDDEN,
                "{path}: {response}"
            );
        }
    }
}
