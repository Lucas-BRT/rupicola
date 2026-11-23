//! Removing a capture. A capture nothing cites goes with a reviewer's word. A cited one goes
//! only where the server and the project allow it, by an owner who confirms what cites it, and
//! it leaves a tombstone so every citation still resolves.

use crate::{
    domain::Access,
    domain::ApiError,
    domain::{Principal, secret::now_ms},
    services::access::access,
    state::Shared,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rupicola_api::{
    CaptureRemoval, Citation, CitationKind, Citations, DeletionPolicy, PolicyView, Role,
};
use sha2::{Digest, Sha256};
use std::sync::atomic::Ordering;

/// The project setting that holds the policy.
const POLICY_KEY: &str = "cited_capture_deletion";
/// The policy of a project that never chose one.
const DEFAULT_POLICY: DeletionPolicy = DeletionPolicy::Confirm;
/// Longest reason a removal may give, in characters.
const MAX_REASON_CHARS: usize = 500;

/// Whether the server lets any project remove a cited capture, from `RUPICOLA_CITED_DELETION`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum CitedDeletion {
    /// Each project's policy decides.
    #[default]
    Allow,
    /// No cited capture is ever removed.
    Never,
}

/// The project's policy as it stands, before the server's ceiling.
async fn project_policy(
    state: &crate::state::AppState,
    project: &str,
) -> Result<DeletionPolicy, ApiError> {
    let stored: Option<String> =
        sqlx::query_scalar("SELECT value FROM project_settings WHERE project_id = ?1 AND key = ?2")
            .bind(project)
            .bind(POLICY_KEY)
            .fetch_optional(&state.db)
            .await?;
    Ok(stored
        .and_then(|value| serde_json::from_str::<String>(&value).ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_POLICY))
}

/// The project of a live capture, after checking the caller may at least read it.
async fn live_capture(
    state: &crate::state::AppState,
    principal: &Principal,
    capture: &str,
) -> Result<(String, Access), ApiError> {
    let project: String =
        sqlx::query_scalar("SELECT project_id FROM captures WHERE id = ?1 AND deleted_at IS NULL")
            .bind(capture)
            .fetch_optional(&state.db)
            .await?
            .ok_or(ApiError::NotFound)?;
    let seen = access(state, principal, &project).await?;
    if let Access::Person(_) = seen {
        seen.require(Role::Reader)?;
    }
    Ok((project, seen))
}

/// Everything that cites `capture`, each once, in a stable order.
async fn cited_by(
    state: &crate::state::AppState,
    capture: &str,
) -> Result<Vec<Citation>, ApiError> {
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "WITH files AS (SELECT id FROM artifacts WHERE capture_id = ?1),
              chosen AS (SELECT DISTINCT selection_id FROM selection_items WHERE capture_id = ?1)
         SELECT 'problem', p.id, p.title, p.status FROM problem_selections ps
           JOIN chosen c ON c.selection_id = ps.selection_id JOIN problems p ON p.id = ps.problem_id
         UNION
         SELECT 'fix', f.id, f.title, f.status FROM fix_selections fs
           JOIN chosen c ON c.selection_id = fs.selection_id JOIN fixes f ON f.id = fs.fix_id
         UNION
         SELECT 'selection', s.id, s.label, NULL FROM selections s JOIN chosen c ON c.selection_id = s.id
           WHERE NOT EXISTS (SELECT 1 FROM problem_selections WHERE selection_id = s.id)
             AND NOT EXISTS (SELECT 1 FROM fix_selections WHERE selection_id = s.id)
         UNION
         SELECT 'fix', f.id, f.title, f.status FROM fix_comparisons k JOIN fixes f ON f.id = k.fix_id
           WHERE k.before_artifact_id IN files OR k.after_artifact_id IN files
              OR k.diff_artifact_id IN files
         UNION
         SELECT 'fix', f.id, f.title, f.status FROM fix_steps t JOIN fixes f ON f.id = t.fix_id
           WHERE t.evidence_artifact_id IN files
         UNION
         SELECT 'design', d.id, d.title, d.status FROM design_entries d WHERE d.evidence IN files
         UNION
         SELECT 'finding', n.id, n.note, NULL FROM capture_findings n
           WHERE n.evidence_artifact_id IN files AND n.capture_id <> ?1",
    )
    .bind(capture)
    .fetch_all(&state.db)
    .await?;
    let mut citations: Vec<Citation> = rows
        .into_iter()
        .map(|(kind, id, title, status)| {
            Ok(Citation {
                kind: kind.parse::<CitationKind>().map_err(anyhow::Error::from)?,
                id,
                title,
                status,
            })
        })
        .collect::<Result<_, ApiError>>()?;
    citations.sort();
    citations.dedup();
    Ok(citations)
}

/// What a removal must send back: a digest of the capture and of what cites it, so a citation
/// added after the caller looked makes the confirmation stale.
fn token(capture: &str, citations: &[Citation]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(capture.as_bytes());
    for citation in citations {
        hasher.update(b"\n");
        hasher.update(citation.kind.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(citation.id.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Whether the caller may remove a capture with these citations under `policy`.
fn allowed(
    state: &crate::state::AppState,
    seen: Access,
    citations: &[Citation],
    policy: DeletionPolicy,
) -> bool {
    match seen {
        Access::Device => false,
        Access::Person(role) if citations.is_empty() => role >= Role::Reviewer,
        Access::Person(role) => {
            role >= Role::Owner
                && policy == DeletionPolicy::Confirm
                && state.cited_deletion.load(Ordering::Relaxed)
        }
    }
}

/// `GET /v1/captures/{id}/citations`: what cites the capture, the captures that descend from it,
/// and what removing it would take.
pub async fn citations(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
) -> Result<Json<Citations>, ApiError> {
    let (project, seen) = live_capture(&state, &principal, &capture).await?;
    let citations = cited_by(&state, &capture).await?;
    let children: Vec<String> = sqlx::query_scalar(
        "SELECT capture_id FROM capture_parents WHERE parent_id = ?1 ORDER BY capture_id",
    )
    .bind(&capture)
    .fetch_all(&state.db)
    .await?;
    let policy = project_policy(&state, &project).await?;
    Ok(Json(Citations {
        allowed: allowed(&state, seen, &citations, policy),
        confirm: (!citations.is_empty()).then(|| token(&capture, &citations)),
        citations,
        children,
        policy,
    }))
}

/// `DELETE /v1/captures/{id}`: removes the capture, leaving a tombstone. A cited capture needs
/// an owner, a policy that allows it and the confirmation the citations returned. An open
/// problem left with no live proof goes back to needs-reprocess.
pub async fn remove(
    State(state): State<Shared>,
    principal: Principal,
    Path(capture): Path<String>,
    Json(form): Json<CaptureRemoval>,
) -> Result<StatusCode, ApiError> {
    let (project, seen) = live_capture(&state, &principal, &capture).await?;
    let actor = principal.person()?.to_owned();
    let reason = form.reason.trim();
    if !(1..=MAX_REASON_CHARS).contains(&reason.chars().count()) {
        return Err(ApiError::invalid(format!(
            "say why the capture goes, in at most {MAX_REASON_CHARS} characters"
        )));
    }
    let citations = cited_by(&state, &capture).await?;
    let policy = project_policy(&state, &project).await?;
    if !allowed(&state, seen, &citations, policy) {
        return Err(ApiError::Forbidden);
    }
    if !citations.is_empty()
        && form.confirm.as_deref() != Some(token(&capture, &citations).as_str())
    {
        return Err(ApiError::Conflict(
            "confirm the citations first: read them again and send back their confirmation",
        ));
    }
    let now = now_ms();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "UPDATE captures SET deleted_at = ?2, deleted_by = ?3, delete_reason = ?4
         WHERE id = ?1 AND deleted_at IS NULL",
    )
    .bind(&capture)
    .bind(now)
    .bind(&actor)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    // An open problem whose every proof now points at removed captures has nothing to show.
    sqlx::query(
        "UPDATE problems SET status = 'needs-reprocess', updated_at = ?2
         WHERE status IN ('open', 'in-progress')
           AND id IN (SELECT ps.problem_id FROM problem_selections ps
                      JOIN selection_items i ON i.selection_id = ps.selection_id
                      WHERE i.capture_id = ?1)
           AND NOT EXISTS (
             SELECT 1 FROM problem_selections ps
             JOIN selection_items i ON i.selection_id = ps.selection_id
             JOIN captures c ON c.id = i.capture_id
             WHERE ps.problem_id = problems.id AND i.mark = 'proof' AND c.deleted_at IS NULL)",
    )
    .bind(&capture)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let detail =
        serde_json::json!({ "capture": capture, "reason": reason, "citations": citations });
    sqlx::query(
        "INSERT INTO audit_events (id, project_id, actor, event, occurred_at, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(&project)
    .bind(&actor)
    .bind(format!("Removed capture {capture}"))
    .bind(now)
    .bind(detail.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/projects/{id}/deletion-policy`
pub async fn policy(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<PolicyView>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Reader)?;
    Ok(Json(PolicyView {
        policy: project_policy(&state, &project).await?,
        server_allows: state.cited_deletion.load(Ordering::Relaxed),
    }))
}

/// `PUT /v1/projects/{id}/deletion-policy`: owners choose whether a cited capture may go.
pub async fn set_policy(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
    Json(form): Json<PolicyView>,
) -> Result<Json<PolicyView>, ApiError> {
    access(&state, &principal, &project)
        .await?
        .require(Role::Owner)?;
    let actor = principal.person()?.to_owned();
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO project_settings (project_id, key, value, generation, updated_by, updated_at)
         VALUES (?1, ?2, ?3, 0, ?4, ?5)
         ON CONFLICT(project_id, key) DO UPDATE SET value = excluded.value,
           generation = generation + 1, updated_by = excluded.updated_by,
           updated_at = excluded.updated_at",
    )
    .bind(&project)
    .bind(POLICY_KEY)
    .bind(serde_json::to_string(form.policy.as_str()).map_err(anyhow::Error::from)?)
    .bind(&actor)
    .bind(now_ms())
    .execute(&mut *tx)
    .await?;
    crate::repositories::audit::record(
        &mut *tx,
        Some(&project),
        &actor,
        &format!("Set the removal of cited captures to {}", form.policy),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(PolicyView {
        policy: form.policy,
        server_allows: state.cited_deletion.load(Ordering::Relaxed),
    }))
}

#[cfg(test)]
mod tests {
    use crate::{state::Shared, testkit::*};
    use axum::http::StatusCode;
    use rupicola_api::{
        Capture, CaptureSummary, CitationKind, Citations, Problem, ProblemStatus,
        record::{ParentKind, ParentRef},
    };
    use serde_json::json;
    use std::sync::atomic::Ordering;

    const A: &str = "01936f2c-aaaa-7bbb-8ccc-0000000007a1";
    const B: &str = "01936f2c-aaaa-7bbb-8ccc-0000000007b1";
    const C: &str = "01936f2c-aaaa-7bbb-8ccc-0000000007c1";

    /// Ana owns `demo`, Bruno reviews and Dani reads. Captures A, B (a child of A) and C; a
    /// problem cites A's file as its only proof. Returns the problem's id too.
    async fn demo() -> (u16, Shared, [String; 3], String) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "dani"] {
            seed_account(&state, handle, false).await;
        }
        let [ana, bruno, dani] = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "dani").await,
        ];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&ana),
            Some(r#"{"id":"demo","label":"Demo"}"#),
        )
        .await;
        for (handle, role) in [("bruno", "reviewer"), ("dani", "reader")] {
            call(
                port,
                "PUT",
                &format!("/v1/projects/demo/members/{handle}"),
                Some(&ana),
                Some(&format!(r#"{{"role":"{role}"}}"#)),
            )
            .await;
        }
        let sha = stream_blob(port, &ana, "demo", b"pixels").await;
        let child_of_a = vec![ParentRef {
            id: A.to_owned(),
            kind: ParentKind::Previous,
        }];
        for (id, parents) in [(A, Vec::new()), (B, child_of_a), (C, Vec::new())] {
            publish_record(port, &ana, &capture_record(id, &sha, 6, parents)).await;
        }
        let capture: Capture =
            json(&call(port, "GET", &format!("/v1/captures/{A}"), Some(&ana), None).await);
        let problem: Problem = json(
            &call(
                port,
                "POST",
                "/v1/projects/demo/problems",
                Some(&ana),
                Some(
                    &json!({ "title": "Low contrast", "severity": "major", "origin": "user-reported",
                             "evidence": [capture.checkpoints[0].artifact.id] })
                    .to_string(),
                ),
            )
            .await,
        );
        (port, state, [ana, bruno, dani], problem.id)
    }

    async fn citations(port: u16, token: &str, capture: &str) -> Citations {
        json(
            &call(
                port,
                "GET",
                &format!("/v1/captures/{capture}/citations"),
                Some(token),
                None,
            )
            .await,
        )
    }

    async fn remove(port: u16, token: &str, capture: &str, confirm: Option<&str>) -> String {
        call(
            port,
            "DELETE",
            &format!("/v1/captures/{capture}"),
            Some(token),
            Some(&json!({ "reason": "wrong build", "confirm": confirm }).to_string()),
        )
        .await
    }

    #[tokio::test]
    async fn a_capture_nothing_cites_goes_with_a_reviewer_and_leaves_a_tombstone() {
        let (port, _, [_, bruno, dani], _) = demo().await;
        let read = citations(port, &bruno, C).await;
        assert!(read.citations.is_empty() && read.allowed && read.confirm.is_none());
        assert_eq!(
            status(&remove(port, &dani, C, None).await),
            StatusCode::FORBIDDEN,
            "a reader"
        );
        let removed = remove(port, &bruno, C, None).await;
        assert_eq!(status(&removed), StatusCode::NO_CONTENT, "{removed}");
        let tomb: Capture =
            json(&call(port, "GET", &format!("/v1/captures/{C}"), Some(&dani), None).await);
        let deleted = tomb.deleted.expect("a tombstone");
        assert_eq!(
            (deleted.by.as_str(), deleted.reason.as_str()),
            ("bruno", "wrong build")
        );
        let listed: Vec<CaptureSummary> =
            json(&call(port, "GET", "/v1/projects/demo/captures", Some(&dani), None).await);
        assert!(
            listed.iter().all(|capture| capture.id != C),
            "a removed capture is not listed"
        );
        assert_eq!(
            status(&remove(port, &bruno, C, None).await),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn a_cited_capture_goes_only_with_an_owner_who_confirms_and_its_problem_reopens() {
        let (port, state, [ana, bruno, _], problem) = demo().await;
        let read = citations(port, &bruno, A).await;
        assert_eq!(read.citations.len(), 1);
        assert_eq!(read.citations[0].kind, CitationKind::Problem);
        assert_eq!(read.children, [B], "a child is listed and does not block");
        assert!(!read.allowed, "a reviewer cannot remove a cited capture");
        assert_eq!(
            status(&remove(port, &bruno, A, read.confirm.as_deref()).await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&remove(port, &ana, A, None).await),
            StatusCode::CONFLICT
        );
        let owner = citations(port, &ana, A).await;
        assert!(owner.allowed);
        let removed = remove(port, &ana, A, owner.confirm.as_deref()).await;
        assert_eq!(status(&removed), StatusCode::NO_CONTENT, "{removed}");
        let reopened: Problem = json(
            &call(
                port,
                "GET",
                &format!("/v1/problems/{problem}"),
                Some(&ana),
                None,
            )
            .await,
        );
        assert_eq!(
            reopened.status,
            ProblemStatus::NeedsReprocess,
            "its only proof is gone"
        );
        assert_eq!(reopened.selections.len(), 1, "the citation still resolves");
        let detail: String = sqlx::query_scalar(
            "SELECT detail FROM audit_events WHERE event LIKE 'Removed capture %'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert!(
            detail.contains(&problem),
            "the audit keeps what cited it: {detail}"
        );
    }

    #[tokio::test]
    async fn a_confirmation_goes_stale_when_a_new_citation_appears() {
        let (port, _, [ana, ..], _) = demo().await;
        let seen = citations(port, &ana, A).await;
        call(
            port,
            "POST",
            "/v1/projects/demo/selections",
            Some(&ana),
            Some(
                &json!({ "label": "Later", "items": [{ "capture": A, "mark": "context" }] })
                    .to_string(),
            ),
        )
        .await;
        let stale = remove(port, &ana, A, seen.confirm.as_deref()).await;
        assert_eq!(status(&stale), StatusCode::CONFLICT, "{stale}");
        let now = citations(port, &ana, A).await;
        assert_eq!(now.citations.len(), 2);
        assert!(
            now.citations
                .iter()
                .any(|c| c.kind == CitationKind::Selection)
        );
    }

    #[tokio::test]
    async fn the_project_and_the_server_can_each_forbid_removing_cited_captures() {
        let (port, state, [ana, bruno, _], _) = demo().await;
        let reviewer = call(
            port,
            "PUT",
            "/v1/projects/demo/deletion-policy",
            Some(&bruno),
            Some(r#"{"policy":"forbid"}"#),
        )
        .await;
        assert_eq!(
            status(&reviewer),
            StatusCode::FORBIDDEN,
            "only owners choose"
        );
        call(
            port,
            "PUT",
            "/v1/projects/demo/deletion-policy",
            Some(&ana),
            Some(r#"{"policy":"forbid"}"#),
        )
        .await;
        let forbidden = citations(port, &ana, A).await;
        assert!(!forbidden.allowed);
        assert_eq!(
            status(&remove(port, &ana, A, forbidden.confirm.as_deref()).await),
            StatusCode::FORBIDDEN
        );
        call(
            port,
            "PUT",
            "/v1/projects/demo/deletion-policy",
            Some(&ana),
            Some(r#"{"policy":"confirm"}"#),
        )
        .await;
        state.cited_deletion.store(false, Ordering::Relaxed);
        let ceiling = citations(port, &ana, A).await;
        assert!(!ceiling.allowed, "the server's never wins over the project");
        assert_eq!(
            status(&remove(port, &ana, A, ceiling.confirm.as_deref()).await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&remove(port, &bruno, C, None).await),
            StatusCode::NO_CONTENT,
            "an uncited capture still goes"
        );
    }
}
