//! Publishing a capture record: the client names the capture, the server checks the record and
//! keeps it once. Sending the same record again is harmless; sending other content under the
//! same id is refused.

use crate::{
    domain::Access,
    domain::ApiError,
    domain::{Principal, secret::now_ms},
    repositories::audit,
    services::access::access,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{
    Capture, Role,
    record::{CaptureRecord, MemberRole},
};
use std::collections::HashMap;

/// `PUT /v1/projects/{id}/captures/{capture}`: keeps the capture record, whose files the
/// project must already hold. 201 when it is new, 200 when the same record was kept before,
/// 409 when the id holds other content.
pub async fn put(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
    Json(record): Json<CaptureRecord>,
) -> Result<(StatusCode, Json<Capture>), ApiError> {
    match access(&state, &principal, &project).await? {
        Access::Device => {}
        person @ Access::Person(_) => person.require(Role::Reviewer)?,
    }
    if record.project != project || record.id != id {
        return Err(ApiError::invalid(format!(
            "the record is capture {} of project {}, not the one the address names",
            record.id, record.project
        )));
    }
    let digest = record
        .digest()
        .map_err(|error| ApiError::invalid(error.to_string()))?;
    let kept: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT project_id, digest FROM captures WHERE id = ?1")
            .bind(&id)
            .fetch_optional(&state.db)
            .await?;
    if let Some((owner, stored)) = kept {
        return if owner == project && stored.as_deref() == Some(digest.as_str()) {
            Ok((
                StatusCode::OK,
                Json(crate::repositories::captures::load(&state.db, &id).await?),
            ))
        } else {
            Err(ApiError::Conflict(
                "a capture with this id already holds other content",
            ))
        };
    }
    check(&state, &project, &record).await?;
    keep(&state, &principal, &project, &record, &digest).await?;
    Ok((
        StatusCode::CREATED,
        Json(crate::repositories::captures::load(&state.db, &id).await?),
    ))
}

/// Checks what only the server knows: the project's limits, the files it holds, and that the
/// parents are its captures.
async fn check(
    state: &crate::state::AppState,
    project: &str,
    record: &CaptureRecord,
) -> Result<(), ApiError> {
    if record
        .members
        .iter()
        .any(|member| member.role == MemberRole::UiSnapshot)
    {
        return Err(ApiError::invalid(
            "this server does not keep UI snapshots yet; publish the capture without them",
        ));
    }
    let limits = crate::services::limits::effective(state, project).await?;
    let most = usize::try_from(limits.max_artifacts_per_capture).unwrap_or(usize::MAX);
    if record.members.len() > most {
        return Err(ApiError::invalid(format!(
            "the capture has {} files; the project allows {most}",
            record.members.len()
        )));
    }
    let hashes: Vec<&str> = record.members.iter().map(|m| m.sha256.as_str()).collect();
    let held: HashMap<String, i64> = sqlx::query_as(
        "SELECT b.sha256, b.bytes FROM project_blobs p JOIN blobs b ON b.sha256 = p.sha256
         WHERE p.project_id = ?1 AND p.sha256 IN (SELECT value FROM json_each(?2))",
    )
    .bind(project)
    .bind(serde_json::to_string(&hashes).map_err(anyhow::Error::from)?)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .collect();
    let missing: Vec<&str> = record
        .members
        .iter()
        .filter(|member| !held.contains_key(&member.sha256))
        .map(|member| member.sha256.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(ApiError::invalid(format!(
            "upload these files to the project first: {}",
            missing.join(", ")
        )));
    }
    if let Some(member) = record
        .members
        .iter()
        .find(|member| i64::try_from(member.bytes).ok() != held.get(&member.sha256).copied())
    {
        return Err(ApiError::invalid(format!(
            "{} says {} bytes, but its file has another size",
            member.path, member.bytes
        )));
    }
    let parents: Vec<&str> = record.parents.iter().map(|p| p.id.as_str()).collect();
    let found: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM captures WHERE project_id = ?1 AND deleted_at IS NULL
           AND id IN (SELECT value FROM json_each(?2))",
    )
    .bind(project)
    .bind(serde_json::to_string(&parents).map_err(anyhow::Error::from)?)
    .fetch_all(&state.db)
    .await?;
    let absent: Vec<&str> = parents
        .into_iter()
        .filter(|parent| !found.iter().any(|id| id == parent))
        .collect();
    if absent.is_empty() {
        Ok(())
    } else {
        Err(ApiError::invalid(format!(
            "publish these parent captures first: {}",
            absent.join(", ")
        )))
    }
}

/// Writes the capture, its files, its parents, its screens and its facets in one transaction.
async fn keep(
    state: &crate::state::AppState,
    principal: &Principal,
    project: &str,
    record: &CaptureRecord,
    digest: &str,
) -> Result<(), ApiError> {
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO captures (id, project_id, scenario, revision, status, blocker, inspection,
           environment, created_at, kind, digest, series, captured_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8, ?9, ?10, ?11, ?12)",
    )
    .bind(&record.id)
    .bind(project)
    .bind(&record.scenario.name)
    .bind(&record.revision)
    .bind(record.status.as_str())
    .bind(&record.blocker)
    .bind(serde_json::to_string(&record.environment).map_err(anyhow::Error::from)?)
    .bind(now_ms())
    .bind(record.kind.as_str())
    .bind(digest)
    .bind(&record.series)
    .bind(record.captured_at)
    .execute(&mut *tx)
    .await?;
    for (position, member) in record.members.iter().enumerate() {
        let surface = match &member.surface {
            Some(key) => Some(surface_id(&mut tx, project, key).await?),
            None => None,
        };
        let locator = if member.locator.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&member.locator).map_err(anyhow::Error::from)?)
        };
        sqlx::query(
            "INSERT INTO artifacts (id, capture_id, role, owner_id, position, path, sha256, bytes,
               mime, target_ms, video_ms, surface_id, locator)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(&record.id)
        .bind(member.role.as_str())
        .bind(&member.owner)
        .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
        .bind(&member.path)
        .bind(&member.sha256)
        .bind(i64::try_from(member.bytes).map_err(anyhow::Error::from)?)
        .bind(crate::domain::blob::mime(&member.path))
        .bind(member.target_ms.and_then(|ms| i64::try_from(ms).ok()))
        .bind(member.video_ms.and_then(|ms| i64::try_from(ms).ok()))
        .bind(surface)
        .bind(locator)
        .execute(&mut *tx)
        .await?;
    }
    for (position, parent) in record.parents.iter().enumerate() {
        sqlx::query(
            "INSERT INTO capture_parents (project_id, capture_id, parent_id, kind, position, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, 'record')",
        )
        .bind(project)
        .bind(&record.id)
        .bind(&parent.id)
        .bind(parent.kind.as_str())
        .bind(i64::try_from(position).map_err(anyhow::Error::from)?)
        .execute(&mut *tx)
        .await?;
    }
    for (key, value) in record.environment.facets() {
        sqlx::query(
            "INSERT INTO capture_facets (project_id, key, value, capture_id) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(project)
        .bind(key)
        .bind(value)
        .bind(&record.id)
        .execute(&mut *tx)
        .await?;
    }
    let actor = principal.handle.as_deref().unwrap_or("device");
    audit::record(
        &mut *tx,
        Some(project),
        actor,
        &format!("Published capture {}", record.scenario.name),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The id of the project's screen called `key`, made the first time it is named.
async fn surface_id(
    tx: &mut sqlx::SqliteConnection,
    project: &str,
    key: &str,
) -> Result<String, ApiError> {
    sqlx::query("INSERT OR IGNORE INTO surfaces (id, project_id, key) VALUES (?1, ?2, ?3)")
        .bind(uuid::Uuid::now_v7().to_string())
        .bind(project)
        .bind(key)
        .execute(&mut *tx)
        .await?;
    Ok(
        sqlx::query_scalar("SELECT id FROM surfaces WHERE project_id = ?1 AND key = ?2")
            .bind(project)
            .bind(key)
            .fetch_one(&mut *tx)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use crate::{state::Shared, testkit::*};
    use axum::http::StatusCode;
    use rupicola_api::{
        Capture, CaptureStatus,
        record::{MemberRole, ParentKind, ParentRef},
    };

    const FIRST: &str = "01936f2c-aaaa-7bbb-8ccc-000000000001";
    const SECOND: &str = "01936f2c-aaaa-7bbb-8ccc-000000000002";
    const HOME: &[u8] = b"home pixels";

    /// Ana owns `demo`, where Bruno reads.
    async fn demo() -> (u16, Shared, [String; 2]) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno] = [sign_in(port, "ana").await, sign_in(port, "bruno").await];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&ana),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        (port, state, [ana, bruno])
    }

    async fn count(state: &crate::state::AppState, sql: &str) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&state.db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_record_is_kept_once_and_the_same_record_again_changes_nothing() {
        let (port, state, [ana, _]) = demo().await;
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        let first = capture_record(FIRST, &sha, 11, Vec::new());
        let created = publish_record(port, &ana, &first).await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let capture: Capture = json(&created);
        assert_eq!(capture.id, FIRST);
        assert_eq!(capture.checkpoints[0].artifact.sha256, sha);
        assert_eq!(
            capture.environment.width,
            Some(800),
            "the views read the older shape"
        );
        assert_eq!(capture.environment.scale, Some(2.0));
        let again = publish_record(port, &ana, &first).await;
        assert_eq!(status(&again), StatusCode::OK, "{again}");
        assert_eq!(count(&state, "SELECT count(*) FROM captures").await, 1);
        let surface = count(
            &state,
            "SELECT count(*) FROM artifacts a JOIN surfaces s ON s.id = a.surface_id
             WHERE s.key = 'home' AND a.locator = '{\"route\":\"/\"}'",
        )
        .await;
        assert_eq!(surface, 1, "the screen and its locator are kept");
        let facets = count(
            &state,
            "SELECT count(*) FROM capture_facets
             WHERE key = 'appearance.color_scheme' AND value = 'dark'",
        )
        .await;
        assert_eq!(facets, 1, "the environment can be searched");
    }

    #[tokio::test]
    async fn other_content_under_a_kept_id_is_a_conflict() {
        let (port, _, [ana, _]) = demo().await;
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        publish_record(port, &ana, &capture_record(FIRST, &sha, 11, Vec::new())).await;
        let mut changed = capture_record(FIRST, &sha, 11, Vec::new());
        changed.captured_at += 1;
        let response = publish_record(port, &ana, &changed).await;
        assert_eq!(status(&response), StatusCode::CONFLICT, "{response}");
    }

    #[tokio::test]
    async fn files_must_be_held_and_their_sizes_true() {
        let (port, _, [ana, _]) = demo().await;
        let absent = "2".repeat(64);
        let response =
            publish_record(port, &ana, &capture_record(FIRST, &absent, 11, Vec::new())).await;
        assert_eq!(
            status(&response),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{response}"
        );
        assert!(body(&response).contains(&absent), "{response}");
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        let wrong_size =
            publish_record(port, &ana, &capture_record(FIRST, &sha, 99, Vec::new())).await;
        assert_eq!(
            status(&wrong_size),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{wrong_size}"
        );
    }

    #[tokio::test]
    async fn a_parent_must_be_published_first_and_then_the_lineage_is_kept() {
        let (port, state, [ana, _]) = demo().await;
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        let parent = vec![ParentRef {
            id: FIRST.to_owned(),
            kind: ParentKind::Previous,
        }];
        let early = publish_record(
            port,
            &ana,
            &capture_record(SECOND, &sha, 11, parent.clone()),
        )
        .await;
        assert_eq!(status(&early), StatusCode::UNPROCESSABLE_ENTITY, "{early}");
        assert!(body(&early).contains(FIRST), "{early}");
        publish_record(port, &ana, &capture_record(FIRST, &sha, 11, Vec::new())).await;
        let child = publish_record(port, &ana, &capture_record(SECOND, &sha, 11, parent)).await;
        assert_eq!(status(&child), StatusCode::CREATED, "{child}");
        let edges = count(
            &state,
            "SELECT count(*) FROM capture_parents WHERE capture_id = '01936f2c-aaaa-7bbb-8ccc-000000000002'
               AND parent_id = '01936f2c-aaaa-7bbb-8ccc-000000000001' AND origin = 'record'",
        )
        .await;
        assert_eq!(edges, 1);
    }

    #[tokio::test]
    async fn a_record_that_does_not_match_its_address_or_rules_is_refused() {
        let (port, _, [ana, bruno]) = demo().await;
        let sha = stream_blob(port, &ana, "demo", HOME).await;
        let first = capture_record(FIRST, &sha, 11, Vec::new());
        let elsewhere = call(
            port,
            "PUT",
            &format!("/v1/projects/demo/captures/{SECOND}"),
            Some(&ana),
            Some(&serde_json::to_string(&first).unwrap()),
        )
        .await;
        assert_eq!(
            status(&elsewhere),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{elsewhere}"
        );
        let mut blocked = first.clone();
        blocked.status = CaptureStatus::Blocked;
        let invalid = publish_record(port, &ana, &blocked).await;
        assert_eq!(
            status(&invalid),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{invalid}"
        );
        assert!(body(&invalid).contains("blocker"), "{invalid}");
        let mut snapshot = first.clone();
        snapshot.members[0].role = MemberRole::UiSnapshot;
        snapshot.members[0].path = "checkpoints/home.json".to_owned();
        let tree = publish_record(port, &ana, &snapshot).await;
        assert_eq!(status(&tree), StatusCode::UNPROCESSABLE_ENTITY, "{tree}");
        let reader = publish_record(port, &bruno, &first).await;
        assert_eq!(status(&reader), StatusCode::FORBIDDEN);
    }
}
