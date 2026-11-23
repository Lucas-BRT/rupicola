//! `/projects/{id}/design`: a project's design records.

use crate::{
    domain::{ApiError, Principal},
    services::design,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{DesignEntry, NewDesignEntry};

/// `GET /v1/projects/{id}/design`, newest first.
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<Vec<DesignEntry>>, ApiError> {
    design::list(&state, &principal, &project).await.map(Json)
}

/// `POST /v1/projects/{id}/design`
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<NewDesignEntry>,
) -> Result<(StatusCode, Json<DesignEntry>), ApiError> {
    let entry = design::create(&state, &principal, &project, form).await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

#[cfg(test)]
mod tests {
    use crate::testkit::*;
    use axum::http::StatusCode;
    use rupicola_api::{DesignEntry, DesignKind, DesignStatus};

    const PATH: &str = "/v1/projects/demo/design";

    fn form(extra: &str) -> String {
        format!(r#"{{"kind":"tactic","status":"adopted","title":"Hairline seams"{extra}}}"#)
    }

    #[tokio::test]
    async fn a_record_is_kept_replaced_and_listed_newest_first() {
        let (port, [ana, bruno, carla]) = demo_with_artifacts().await;
        let first = call(port, "POST", PATH, Some(&ana), Some(&form(""))).await;
        assert_eq!(status(&first), StatusCode::CREATED, "{first}");
        let first: DesignEntry = json(&first);
        assert_eq!(first.entry.kind, DesignKind::Tactic);
        assert_eq!(first.entry.status, DesignStatus::Adopted);
        assert_eq!(first.author.as_deref(), Some("ana"));

        let extra = format!(
            r#","supersedes":"{}","evidence":"{PROJECT_ARTIFACT}""#,
            first.id
        );
        let second = call(port, "POST", PATH, Some(&ana), Some(&form(&extra))).await;
        assert_eq!(status(&second), StatusCode::CREATED, "{second}");

        let listed: Vec<DesignEntry> = json(&call(port, "GET", PATH, Some(&bruno), None).await);
        assert_eq!(listed.len(), 2);
        assert_eq!(
            listed[0].entry.supersedes.as_deref(),
            Some(first.id.as_str())
        );
        assert_eq!(listed[0].entry.evidence.as_deref(), Some(PROJECT_ARTIFACT));

        assert_eq!(
            status(&call(port, "POST", PATH, Some(&bruno), Some(&form(""))).await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&call(port, "GET", PATH, Some(&carla), None).await),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn a_record_cannot_cite_another_projects_files_or_records() {
        let (port, [ana, ..]) = demo_with_artifacts().await;
        for extra in [
            format!(r#","evidence":"{FOREIGN_ARTIFACT}""#),
            r#","supersedes":"missing""#.to_owned(),
        ] {
            let refused = call(port, "POST", PATH, Some(&ana), Some(&form(&extra))).await;
            assert!(status(&refused).is_client_error(), "{refused}");
        }
        let blank = call(
            port,
            "POST",
            PATH,
            Some(&ana),
            Some(r#"{"kind":"brief","status":"proposed","title":"  "}"#),
        )
        .await;
        assert_eq!(status(&blank), StatusCode::UNPROCESSABLE_ENTITY, "{blank}");
    }
}
