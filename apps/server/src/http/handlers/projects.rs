//! `/projects` and `/projects/{id}`: the projects, their members and their audit trail.

use crate::{
    domain::{ApiError, Principal},
    services::projects,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{AuditEvent, Member, MemberRole, NewProject, Project, ProjectUpdate};

/// `GET /v1/projects`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
) -> Result<Json<Vec<Project>>, ApiError> {
    projects::list(&state, &principal).await.map(Json)
}

/// `POST /v1/projects`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Json(form): Json<NewProject>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    let project = projects::create(&state, &principal, form).await?;
    Ok((StatusCode::CREATED, Json(project)))
}

/// `GET /v1/projects/{id}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Project>, ApiError> {
    projects::get(&state, &principal, &id).await.map(Json)
}

/// `PATCH /v1/projects/{id}`
pub async fn update(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
    Json(form): Json<ProjectUpdate>,
) -> Result<Json<Project>, ApiError> {
    projects::rename(&state, &principal, &id, &form.label)
        .await
        .map(Json)
}

/// `DELETE /v1/projects/{id}`
pub async fn delete(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    projects::delete(&state, &principal, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/projects/{id}/members`
pub async fn members(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Vec<Member>>, ApiError> {
    projects::members(&state, &principal, &id).await.map(Json)
}

/// `PUT /v1/projects/{id}/members/{handle}`
pub async fn put_member(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, handle)): Path<(String, String)>,
    Json(form): Json<MemberRole>,
) -> Result<Json<Member>, ApiError> {
    projects::put_member(&state, &principal, &id, &handle, form.role)
        .await
        .map(Json)
}

/// `DELETE /v1/projects/{id}/members/{handle}`
pub async fn delete_member(
    State(state): State<Shared>,
    principal: Principal,
    Path((id, handle)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    projects::delete_member(&state, &principal, &id, &handle).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/projects/{id}/audit`
pub async fn audit_log(
    State(state): State<Shared>,
    principal: Principal,
    Path(id): Path<String>,
) -> Result<Json<Vec<AuditEvent>>, ApiError> {
    projects::audit_log(&state, &principal, &id).await.map(Json)
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{AuditEvent, Member, Project, Role};

    /// Ana (no special rights) owns project `demo`; Bruno has an account only.
    async fn demo() -> (u16, String, String) {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        seed_account(&state, "bruno", false).await;
        let ana = sign_in(port, "ana").await;
        let bruno = sign_in(port, "bruno").await;
        let created = call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":" Demo "}"#),
        )
        .await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        (port, ana, bruno)
    }

    #[tokio::test]
    async fn the_creator_owns_the_project_and_outsiders_cannot_see_it() {
        let (port, ana, bruno) = demo().await;

        let project: Project =
            json(&call(port, "GET", "/v1/projects/demo", Some(&ana), None).await);
        assert_eq!(project.label, "Demo");
        assert_eq!(project.role, Some(Role::Owner));
        let mine: Vec<Project> = json(&call(port, "GET", "/v1/projects", Some(&ana), None).await);
        assert_eq!(mine.len(), 1);

        // To an outsider the project does not exist: 404, not 403.
        let hidden = call(port, "GET", "/v1/projects/demo", Some(&bruno), None).await;
        assert_eq!(status(&hidden), StatusCode::NOT_FOUND, "{hidden}");
        let theirs: Vec<Project> =
            json(&call(port, "GET", "/v1/projects", Some(&bruno), None).await);
        assert_eq!(theirs, []);

        let duplicate = call(
            port,
            "POST",
            "/v1/projects",
            Some(&bruno),
            Some(r#"{"id":"demo","label":"Mine"}"#),
        )
        .await;
        assert_eq!(status(&duplicate), StatusCode::CONFLICT, "{duplicate}");
        let bad = call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"../x","label":"X"}"#),
        )
        .await;
        assert_eq!(status(&bad), StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    #[tokio::test]
    async fn roles_limit_what_members_may_change() {
        let (port, ana, bruno) = demo().await;
        let added = call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        assert_eq!(status(&added), StatusCode::OK, "{added}");

        let seen: Project = json(&call(port, "GET", "/v1/projects/demo", Some(&bruno), None).await);
        assert_eq!(seen.role, Some(Role::Reader));
        let rename = call(
            port,
            "PATCH",
            "/v1/projects/demo",
            Some(&bruno),
            Some(r#"{"label":"Taken"}"#),
        )
        .await;
        assert_eq!(status(&rename), StatusCode::FORBIDDEN, "{rename}");
        let promote = call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&bruno),
            Some(r#"{"role":"owner"}"#),
        )
        .await;
        assert_eq!(status(&promote), StatusCode::FORBIDDEN, "{promote}");

        let members: Vec<Member> =
            json(&call(port, "GET", "/v1/projects/demo/members", Some(&bruno), None).await);
        assert_eq!(
            members,
            [
                Member {
                    handle: "ana".into(),
                    role: Role::Owner
                },
                Member {
                    handle: "bruno".into(),
                    role: Role::Reader
                },
            ]
        );
        let ghost = call(
            port,
            "PUT",
            "/v1/projects/demo/members/nobody",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        assert_eq!(status(&ghost), StatusCode::NOT_FOUND, "{ghost}");
    }

    #[tokio::test]
    async fn a_project_always_keeps_an_owner() {
        let (port, ana, _) = demo().await;
        let demote = call(
            port,
            "PUT",
            "/v1/projects/demo/members/ana",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        assert_eq!(status(&demote), StatusCode::CONFLICT, "{demote}");
        let leave = call(
            port,
            "DELETE",
            "/v1/projects/demo/members/ana",
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&leave), StatusCode::CONFLICT, "{leave}");

        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"owner"}"#),
        )
        .await;
        let leave = call(
            port,
            "DELETE",
            "/v1/projects/demo/members/ana",
            Some(&ana),
            None,
        )
        .await;
        assert_eq!(status(&leave), StatusCode::NO_CONTENT, "{leave}");
    }

    #[tokio::test]
    async fn owners_rename_and_delete_with_an_audit_trail() {
        let (port, ana, _) = demo().await;
        let renamed: Project = json(
            &call(
                port,
                "PATCH",
                "/v1/projects/demo",
                Some(&ana),
                Some(r#"{"label":"Renamed"}"#),
            )
            .await,
        );
        assert_eq!(renamed.label, "Renamed");

        let events: Vec<AuditEvent> =
            json(&call(port, "GET", "/v1/projects/demo/audit", Some(&ana), None).await);
        let described: Vec<_> = events.iter().map(|event| event.event.as_str()).collect();
        assert_eq!(
            described,
            ["Renamed the project to Renamed", "Created the project"]
        );
        assert!(events.iter().all(|event| event.actor == "ana"));

        let deleted = call(port, "DELETE", "/v1/projects/demo", Some(&ana), None).await;
        assert_eq!(status(&deleted), StatusCode::NO_CONTENT, "{deleted}");
        let gone = call(port, "GET", "/v1/projects/demo", Some(&ana), None).await;
        assert_eq!(status(&gone), StatusCode::NOT_FOUND, "{gone}");
    }

    #[tokio::test]
    async fn an_administrator_sees_every_project_as_owner() {
        let (port, state) = start_custom(settings(), None, true).await;
        seed_account(&state, "ana", false).await;
        seed_account(&state, "root", true).await;
        let ana = sign_in(port, "ana").await;
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        let root = sign_in(port, "root").await;

        let all: Vec<Project> = json(&call(port, "GET", "/v1/projects", Some(&root), None).await);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].role, Some(Role::Owner));
    }
}
