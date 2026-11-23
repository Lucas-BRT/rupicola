//! Design systems: versions of a project's tokens and guidance, reviewed and adopted by its
//! owner, with example pages and the browser validation that must pass before review.

mod browser;
mod document;

use crate::{
    auth::{Principal, now_ms},
    error::ApiError,
    projects::access,
    state::Shared,
};
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::Response,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use browser::{Report, validate};
use document::{fingerprint, render_screen, valid};
use rupicola_api::{
    Document, DocumentEdit, Role, SystemAction, SystemActionForm, SystemItem, SystemList,
    SystemNote, SystemPhase, SystemReport, SystemVersion,
};
use sqlx::Row;

/// Versions the list shows.
const LIST_LIMIT: i64 = 100;
/// Notes one version may collect.
const MAX_NOTES: usize = 1000;
/// Longest note, in bytes.
const MAX_NOTE_BYTES: usize = 4000;
/// Parts of the document a comment may be about; any other target means the whole document.
const TARGETS: [&str; 6] = [
    "identity",
    "tokens",
    "components",
    "comparison",
    "media-unavailable",
    "form",
];
/// Seconds a caller is told to wait while another validation runs.
const VALIDATION_RETRY_SECS: u64 = 5;

/// The routes of the design systems.
pub fn routes() -> axum::Router<Shared> {
    use axum::routing;
    axum::Router::new()
        .route(
            "/projects/{id}/design-systems",
            routing::get(list).post(create),
        )
        .route("/projects/{id}/design-systems/{version}", routing::get(get))
        .route(
            "/projects/{id}/design-systems/{version}/document",
            routing::put(edit),
        )
        .route(
            "/projects/{id}/design-systems/{version}/actions",
            routing::post(act),
        )
        .route(
            "/projects/{id}/design-systems/{version}/preview",
            routing::get(preview),
        )
        .route(
            "/projects/{id}/design-systems/{version}/proofs/{name}",
            routing::get(proof),
        )
}

/// A version as stored.
struct Stored {
    id: String,
    generation: i64,
    parent: Option<String>,
    author: String,
    phase: SystemPhase,
    document: Document,
    notes: Vec<SystemNote>,
    report: Option<Report>,
    frozen_screen: Option<String>,
}

impl Stored {
    fn draft(parent: Option<String>, author: String, document: Document) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            generation: 0,
            parent,
            author,
            phase: SystemPhase::Draft,
            document,
            notes: Vec::new(),
            report: None,
            frozen_screen: None,
        }
    }

    /// Whether the validation covers this document and the adopted version.
    fn ready(&self, adopted: Option<&str>) -> bool {
        self.report.as_ref().is_some_and(|report| {
            report.passed()
                && report.baseline.as_deref() == adopted
                && report.fingerprint == fingerprint(&self.document)
        })
    }

    fn submit(&mut self, adopted: Option<&str>) -> Result<(), ApiError> {
        if self.phase != SystemPhase::Draft || !self.ready(adopted) {
            return Err(ApiError::Conflict(
                "validate this version again before sending it for review",
            ));
        }
        self.frozen_screen = Some(render_screen(&self.document));
        self.phase = SystemPhase::Review;
        Ok(())
    }

    fn note(&mut self, actor: &str, action: &str, target: &str, text: String) {
        self.notes.push(SystemNote {
            actor: actor.to_owned(),
            time: now_ms() / 1000,
            action: action.to_owned(),
            target: target.to_owned(),
            text,
        });
    }

    fn dto(self, adopted: Option<&str>) -> SystemVersion {
        let ready = self.ready(adopted);
        SystemVersion {
            id: self.id,
            parent: self.parent,
            author: self.author,
            phase: self.phase,
            generation: self.generation,
            document: self.document,
            notes: self.notes,
            report: self.report.map(|report| SystemReport {
                outcome: report.outcome,
                browser: report.browser,
                checks: report.checks,
                shots: report.shots.into_iter().map(|shot| shot.name).collect(),
                error: report.error,
                baseline: report.baseline,
            }),
            ready,
        }
    }
}

fn corrupt(error: impl Into<anyhow::Error>) -> ApiError {
    ApiError::Internal(error.into())
}

/// The version of `project` that is adopted.
async fn adopted(state: &Shared, project: &str) -> Result<Option<String>, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT version_id FROM design_adoption WHERE project_id = ?1")
            .bind(project)
            .fetch_optional(&state.db)
            .await?,
    )
}

async fn read(state: &Shared, project: &str, id: &str) -> Result<Stored, ApiError> {
    let row = sqlx::query(
        "SELECT parent_id, author, phase, generation, document, notes, report, frozen_screen
         FROM design_versions WHERE id = ?1 AND project_id = ?2",
    )
    .bind(id)
    .bind(project)
    .fetch_optional(&state.db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let phase: String = row.get("phase");
    let document: String = row.get("document");
    let notes: String = row.get("notes");
    let report: Option<String> = row.get("report");
    Ok(Stored {
        id: id.to_owned(),
        generation: row.get("generation"),
        parent: row.get("parent_id"),
        author: row.get("author"),
        phase: phase.parse().map_err(corrupt)?,
        document: serde_json::from_str(&document).map_err(corrupt)?,
        notes: serde_json::from_str(&notes).map_err(corrupt)?,
        report: report
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(corrupt)?,
        frozen_screen: row.get("frozen_screen"),
    })
}

async fn insert(state: &Shared, project: &str, version: &Stored) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO design_versions
           (id, project_id, parent_id, author, phase, generation, name, document, notes, report,
            frozen_screen, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, NULL, NULL, ?9)",
    )
    .bind(&version.id)
    .bind(project)
    .bind(&version.parent)
    .bind(&version.author)
    .bind(version.phase.as_str())
    .bind(&version.document.name)
    .bind(serde_json::to_string(&version.document).map_err(corrupt)?)
    .bind(serde_json::to_string(&version.notes).map_err(corrupt)?)
    .bind(now_ms())
    .execute(&state.db)
    .await?;
    Ok(())
}

const SAVE: &str = "UPDATE design_versions
    SET generation = generation + 1, name = ?1, phase = ?2, document = ?3, notes = ?4,
        report = ?5, frozen_screen = ?6
    WHERE id = ?7 AND project_id = ?8 AND generation = ?9";

/// Writes `version` if nobody changed it since it was read.
async fn save(state: &Shared, project: &str, version: &Stored) -> Result<(), ApiError> {
    let written = sqlx::query(SAVE)
        .bind(&version.document.name)
        .bind(version.phase.as_str())
        .bind(serde_json::to_string(&version.document).map_err(corrupt)?)
        .bind(serde_json::to_string(&version.notes).map_err(corrupt)?)
        .bind(
            version
                .report
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(corrupt)?,
        )
        .bind(&version.frozen_screen)
        .bind(&version.id)
        .bind(project)
        .bind(version.generation)
        .execute(&state.db)
        .await?
        .rows_affected();
    if written == 1 {
        Ok(())
    } else {
        Err(ApiError::Conflict("the version changed; reload it"))
    }
}

async fn require(
    state: &Shared,
    principal: &Principal,
    project: &str,
    role: Role,
) -> Result<(), ApiError> {
    access(state, principal, project).await?.require(role)
}

/// `GET /v1/projects/{id}/design-systems`
pub async fn list(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<Json<SystemList>, ApiError> {
    require(&state, &principal, &project, Role::Reader).await?;
    let rows = sqlx::query(
        "SELECT id, name, phase FROM design_versions WHERE project_id = ?1
         ORDER BY created_at DESC, rowid DESC LIMIT ?2",
    )
    .bind(&project)
    .bind(LIST_LIMIT)
    .fetch_all(&state.db)
    .await?;
    let items = rows
        .into_iter()
        .map(|row| {
            let phase: String = row.get("phase");
            Ok(SystemItem {
                id: row.get("id"),
                name: row.get("name"),
                phase: phase.parse().map_err(corrupt)?,
            })
        })
        .collect::<Result<_, ApiError>>()?;
    Ok(Json(SystemList {
        items,
        active: adopted(&state, &project).await?,
    }))
}

/// `GET /v1/projects/{id}/design-systems/{version}`
pub async fn get(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
) -> Result<Json<SystemVersion>, ApiError> {
    require(&state, &principal, &project, Role::Reader).await?;
    let active = adopted(&state, &project).await?;
    Ok(Json(
        read(&state, &project, &id).await?.dto(active.as_deref()),
    ))
}

/// `POST /v1/projects/{id}/design-systems`: a new draft, starting from the adopted version.
pub async fn create(
    State(state): State<Shared>,
    principal: Principal,
    Path(project): Path<String>,
) -> Result<(StatusCode, Json<SystemVersion>), ApiError> {
    require(&state, &principal, &project, Role::Owner).await?;
    let active = adopted(&state, &project).await?;
    let document = match active.as_deref() {
        Some(id) => read(&state, &project, id).await?.document,
        None => Document::default(),
    };
    let draft = Stored::draft(active.clone(), author(&principal), document);
    insert(&state, &project, &draft).await?;
    Ok((StatusCode::CREATED, Json(draft.dto(active.as_deref()))))
}

fn author(principal: &Principal) -> String {
    principal
        .handle
        .clone()
        .unwrap_or_else(|| "device".to_owned())
}

/// `PUT /v1/projects/{id}/design-systems/{version}/document`
pub async fn edit(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
    Json(form): Json<DocumentEdit>,
) -> Result<Json<SystemVersion>, ApiError> {
    require(&state, &principal, &project, Role::Owner).await?;
    if !valid(&form.document) {
        return Err(ApiError::invalid(
            "the document is incomplete or contains invalid tokens",
        ));
    }
    let mut version = read(&state, &project, &id).await?;
    if version.phase != SystemPhase::Draft || version.generation != form.generation {
        return Err(ApiError::Conflict("only the latest draft can be edited"));
    }
    version.document = form.document;
    version.report = None;
    save(&state, &project, &version).await?;
    let active = adopted(&state, &project).await?;
    Ok(Json(
        read(&state, &project, &id).await?.dto(active.as_deref()),
    ))
}

/// `POST /v1/projects/{id}/design-systems/{version}/actions`
pub async fn act(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
    Json(form): Json<SystemActionForm>,
) -> Result<Json<SystemVersion>, ApiError> {
    require(&state, &principal, &project, Role::Owner).await?;
    let actor = author(&principal);
    let mut version = read(&state, &project, &id).await?;
    if version.generation != form.generation {
        return Err(ApiError::Conflict("the version changed; reload it"));
    }
    let active = adopted(&state, &project).await?;
    let result = match form.action {
        SystemAction::Validate => {
            validate_version(&state, &project, &mut version, active.as_deref()).await?;
            version
        }
        SystemAction::Submit => {
            version.submit(active.as_deref())?;
            save(&state, &project, &version).await?;
            version
        }
        SystemAction::Variation => {
            let new = Stored::draft(Some(version.id), actor, version.document);
            insert(&state, &project, &new).await?;
            new
        }
        SystemAction::Adopt => {
            adopt(&state, &project, &mut version, &actor, active.as_deref()).await?;
            version
        }
        SystemAction::Comment | SystemAction::Approve | SystemAction::Changes => {
            review(&mut version, &actor, &form)?;
            save(&state, &project, &version).await?;
            version
        }
    };
    let active = adopted(&state, &project).await?;
    let stored = read(&state, &project, &result.id).await?;
    Ok(Json(stored.dto(active.as_deref())))
}

/// Runs the browser checks on a draft and keeps the report with it.
async fn validate_version(
    state: &Shared,
    project: &str,
    version: &mut Stored,
    active: Option<&str>,
) -> Result<(), ApiError> {
    if version.phase != SystemPhase::Draft {
        return Err(ApiError::Conflict("only a draft can be validated"));
    }
    let baseline = match active {
        Some(id) => {
            let mut doc = read(state, project, id).await?.document;
            let draft = &version.document;
            for (to, from) in [
                (&mut doc.name, &draft.name),
                (&mut doc.brand, &draft.brand),
                (&mut doc.landing_title, &draft.landing_title),
                (&mut doc.queue_title, &draft.queue_title),
                (&mut doc.queue_item, &draft.queue_item),
                (&mut doc.settings_title, &draft.settings_title),
                (&mut doc.screen_title, &draft.screen_title),
                (&mut doc.scenario, &draft.scenario),
                (&mut doc.before_label, &draft.before_label),
                (&mut doc.after_label, &draft.after_label),
            ] {
                to.clone_from(from);
            }
            Some(render_screen(&doc))
        }
        None => None,
    };
    let _busy = state
        .design_validation
        .try_acquire()
        .map_err(|_| ApiError::TooManyRequests {
            retry_after: VALIDATION_RETRY_SECS,
        })?;
    let document = version.document.clone();
    let mut report = tokio::task::spawn_blocking(move || validate(&document, baseline.as_deref()))
        .await
        .map_err(corrupt)?;
    if adopted(state, project).await?.as_deref() != active {
        return Err(ApiError::Conflict(
            "another system was adopted meanwhile; validate again",
        ));
    }
    report.baseline = active.map(str::to_owned);
    version.report = Some(report);
    save(state, project, version).await
}

fn review(version: &mut Stored, actor: &str, form: &SystemActionForm) -> Result<(), ApiError> {
    if form.note.len() > MAX_NOTE_BYTES || version.notes.len() >= MAX_NOTES {
        return Err(ApiError::invalid(
            "the note is too long or the version has too many",
        ));
    }
    let text = form.note.trim();
    let target = if TARGETS.contains(&form.target.as_str()) {
        form.target.as_str()
    } else {
        "document"
    };
    match form.action {
        SystemAction::Comment if version.phase != SystemPhase::Draft && !text.is_empty() => (),
        SystemAction::Approve if version.phase == SystemPhase::Review => {
            version.phase = SystemPhase::Approved;
        }
        SystemAction::Changes if version.phase == SystemPhase::Review && !text.is_empty() => {
            version.phase = SystemPhase::Changes;
        }
        _ => return Err(ApiError::Conflict("that cannot be done in this phase")),
    }
    version.note(actor, form.action.as_str(), target, text.to_owned());
    Ok(())
}

async fn adopt(
    state: &Shared,
    project: &str,
    version: &mut Stored,
    actor: &str,
    active: Option<&str>,
) -> Result<(), ApiError> {
    if version.phase != SystemPhase::Approved || version.frozen_screen.is_none() {
        return Err(ApiError::Conflict(
            "only an approved version can be adopted",
        ));
    }
    if active == Some(version.id.as_str()) {
        return Ok(());
    }
    if version.notes.len() >= MAX_NOTES {
        return Err(ApiError::invalid("the version has too many notes"));
    }
    version.note(actor, "adopt", "document", String::new());
    let mut tx = state.db.begin().await?;
    let written = sqlx::query(SAVE)
        .bind(&version.document.name)
        .bind(version.phase.as_str())
        .bind(serde_json::to_string(&version.document).map_err(corrupt)?)
        .bind(serde_json::to_string(&version.notes).map_err(corrupt)?)
        .bind(
            version
                .report
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(corrupt)?,
        )
        .bind(&version.frozen_screen)
        .bind(&version.id)
        .bind(project)
        .bind(version.generation)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if written != 1 {
        return Err(ApiError::Conflict("the version changed; reload it"));
    }
    sqlx::query(
        "INSERT INTO design_adoption (project_id, version_id) VALUES (?1, ?2)
         ON CONFLICT(project_id) DO UPDATE SET version_id = excluded.version_id",
    )
    .bind(project)
    .bind(&version.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// `GET /v1/projects/{id}/design-systems/{version}/preview`: the example pages. A client that
/// shows them in a frame sets the policy for that.
pub async fn preview(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    require(&state, &principal, &project, Role::Reader).await?;
    let version = read(&state, &project, &id).await?;
    let html = version
        .frozen_screen
        .unwrap_or_else(|| render_screen(&version.document));
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .map_err(corrupt)
}

/// `GET /v1/projects/{id}/design-systems/{version}/proofs/{name}`: one validation screenshot.
pub async fn proof(
    State(state): State<Shared>,
    principal: Principal,
    Path((project, id, name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    require(&state, &principal, &project, Role::Reader).await?;
    let png = read(&state, &project, &id)
        .await?
        .report
        .and_then(|report| report.shots.into_iter().find(|shot| shot.name == name))
        .map(|shot| shot.png)
        .ok_or(ApiError::NotFound)?;
    let bytes = STANDARD.decode(png).map_err(corrupt)?;
    Response::builder()
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(bytes))
        .map_err(corrupt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::*;
    use rupicola_api::{SystemList, SystemVersion};

    const BASE: &str = "/v1/projects/demo/design-systems";

    async fn setup() -> (u16, crate::state::Shared, [String; 3]) {
        let (port, state) = start_custom(settings(), None, true).await;
        for handle in ["ana", "bruno", "carla"] {
            seed_account(&state, handle, false).await;
        }
        let tokens = [
            sign_in(port, "ana").await,
            sign_in(port, "bruno").await,
            sign_in(port, "carla").await,
        ];
        call(
            port,
            "POST",
            "/v1/projects",
            Some(&tokens[0]),
            Some(r#"{"id":"demo","label":"demo"}"#),
        )
        .await;
        call(
            port,
            "PUT",
            "/v1/projects/demo/members/bruno",
            Some(&tokens[0]),
            Some(r#"{"role":"reader"}"#),
        )
        .await;
        (port, state, tokens)
    }

    async fn act_on(port: u16, token: &str, id: &str, body: &str) -> String {
        call(
            port,
            "POST",
            &format!("{BASE}/{id}/actions"),
            Some(token),
            Some(body),
        )
        .await
    }

    /// Stores a passing validation, as the browser would after checking the document.
    async fn pass(state: &crate::state::Shared, version: &SystemVersion, baseline: Option<&str>) {
        let report = Report {
            fingerprint: fingerprint(&version.document),
            baseline: baseline.map(str::to_owned),
            outcome: rupicola_api::Outcome::Passed,
            browser: Some("test".into()),
            checks: vec![],
            shots: vec![],
            error: None,
        };
        sqlx::query(
            "UPDATE design_versions SET report = ?1, generation = generation + 1 WHERE id = ?2",
        )
        .bind(serde_json::to_string(&report).unwrap())
        .bind(&version.id)
        .execute(&state.db)
        .await
        .unwrap();
    }

    async fn fetch(port: u16, token: &str, id: &str) -> SystemVersion {
        json(&call(port, "GET", &format!("{BASE}/{id}"), Some(token), None).await)
    }

    #[tokio::test]
    async fn only_an_owner_writes_and_members_read() {
        let (port, _state, [ana, bruno, carla]) = setup().await;
        let created = call(port, "POST", BASE, Some(&ana), None).await;
        assert_eq!(status(&created), StatusCode::CREATED, "{created}");
        let draft: SystemVersion = json(&created);
        assert_eq!(draft.phase, SystemPhase::Draft);
        assert_eq!(draft.author, "ana");
        assert!(!draft.ready);

        let listed: SystemList = json(&call(port, "GET", BASE, Some(&bruno), None).await);
        assert_eq!(listed.items.len(), 1);
        assert_eq!(listed.active, None);
        assert_eq!(fetch(port, &bruno, &draft.id).await.id, draft.id);
        assert_eq!(
            status(&call(port, "POST", BASE, Some(&bruno), None).await),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status(&call(port, "GET", BASE, Some(&carla), None).await),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(&call(port, "GET", &format!("{BASE}/missing"), Some(&ana), None).await),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn an_edit_needs_a_valid_document_and_the_latest_generation() {
        let (port, _state, [ana, ..]) = setup().await;
        let draft: SystemVersion = json(&call(port, "POST", BASE, Some(&ana), None).await);
        let edit = |document: &Document, generation: i64| {
            serde_json::to_string(&DocumentEdit {
                generation,
                document: document.clone(),
            })
            .unwrap()
        };
        let path = format!("{BASE}/{}/document", draft.id);
        let bad = Document {
            accent: "#f0f; };".into(),
            ..draft.document.clone()
        };
        let refused = call(port, "PUT", &path, Some(&ana), Some(&edit(&bad, 0))).await;
        assert_eq!(
            status(&refused),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{refused}"
        );

        let renamed = Document {
            name: "Renamed".into(),
            ..draft.document.clone()
        };
        let saved = call(port, "PUT", &path, Some(&ana), Some(&edit(&renamed, 0))).await;
        assert_eq!(status(&saved), StatusCode::OK, "{saved}");
        let saved: SystemVersion = json(&saved);
        assert_eq!(saved.document.name, "Renamed");
        assert_eq!(saved.generation, 1);
        let stale = call(port, "PUT", &path, Some(&ana), Some(&edit(&renamed, 0))).await;
        assert_eq!(status(&stale), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_version_is_validated_before_review_and_approved_before_adoption() {
        let (port, state, [ana, ..]) = setup().await;
        let draft: SystemVersion = json(&call(port, "POST", BASE, Some(&ana), None).await);

        let early = act_on(
            port,
            &ana,
            &draft.id,
            r#"{"action":"submit","generation":0}"#,
        )
        .await;
        assert_eq!(status(&early), StatusCode::CONFLICT, "unvalidated: {early}");
        let comment = act_on(
            port,
            &ana,
            &draft.id,
            r#"{"action":"comment","generation":0,"note":"x"}"#,
        )
        .await;
        assert_eq!(
            status(&comment),
            StatusCode::CONFLICT,
            "drafts take no comments"
        );

        pass(&state, &draft, None).await;
        let ready = fetch(port, &ana, &draft.id).await;
        assert!(ready.ready);
        let sent = act_on(
            port,
            &ana,
            &draft.id,
            &format!(r#"{{"action":"submit","generation":{}}}"#, ready.generation),
        )
        .await;
        assert_eq!(status(&sent), StatusCode::OK, "{sent}");
        let sent: SystemVersion = json(&sent);
        assert_eq!(sent.phase, SystemPhase::Review);

        let not_yet = act_on(
            port,
            &ana,
            &draft.id,
            &format!(r#"{{"action":"adopt","generation":{}}}"#, sent.generation),
        )
        .await;
        assert_eq!(
            status(&not_yet),
            StatusCode::CONFLICT,
            "adoption needs approval"
        );
        let blank = act_on(
            port,
            &ana,
            &draft.id,
            &format!(r#"{{"action":"changes","generation":{}}}"#, sent.generation),
        )
        .await;
        assert_eq!(
            status(&blank),
            StatusCode::CONFLICT,
            "changes need a reason"
        );
        let approved: SystemVersion = json(
            &act_on(
                port,
                &ana,
                &draft.id,
                &format!(r#"{{"action":"approve","generation":{}}}"#, sent.generation),
            )
            .await,
        );
        assert_eq!(approved.phase, SystemPhase::Approved);
        adopt_and_vary(port, &ana, &draft, &approved).await;
    }

    async fn adopt_and_vary(port: u16, ana: &str, draft: &SystemVersion, approved: &SystemVersion) {
        let adopted = act_on(
            port,
            ana,
            &draft.id,
            &format!(
                r#"{{"action":"adopt","generation":{}}}"#,
                approved.generation
            ),
        )
        .await;
        assert_eq!(status(&adopted), StatusCode::OK, "{adopted}");
        let adopted: SystemVersion = json(&adopted);
        assert_eq!(
            adopted.notes.last().map(|n| n.action.as_str()),
            Some("adopt")
        );

        let again = act_on(
            port,
            ana,
            &draft.id,
            &format!(
                r#"{{"action":"adopt","generation":{}}}"#,
                adopted.generation
            ),
        )
        .await;
        assert_eq!(
            status(&again),
            StatusCode::OK,
            "adopting twice changes nothing"
        );
        assert_eq!(
            json::<SystemVersion>(&again).notes.len(),
            adopted.notes.len()
        );
        let listed: SystemList = json(&call(port, "GET", BASE, Some(ana), None).await);
        assert_eq!(listed.active.as_deref(), Some(draft.id.as_str()));

        let next: SystemVersion = json(&call(port, "POST", BASE, Some(ana), None).await);
        assert_eq!(next.parent.as_deref(), Some(draft.id.as_str()));
        let varied: SystemVersion = json(
            &act_on(
                port,
                ana,
                &next.id,
                r#"{"action":"variation","generation":0}"#,
            )
            .await,
        );
        assert_eq!(varied.parent.as_deref(), Some(next.id.as_str()));
    }

    #[tokio::test]
    async fn the_preview_and_proofs_are_served_to_members() {
        let (port, state, [ana, bruno, _]) = setup().await;
        let draft: SystemVersion = json(&call(port, "POST", BASE, Some(&ana), None).await);
        let page = call(
            port,
            "GET",
            &format!("{BASE}/{}/preview", draft.id),
            Some(&bruno),
            None,
        )
        .await;
        assert_eq!(status(&page), StatusCode::OK);
        assert!(body(&page).contains("--accent:#ff681e"));
        assert!(header(&page, "content-type").is_some_and(|kind| kind.starts_with("text/html")));
        let missing = call(
            port,
            "GET",
            &format!("{BASE}/{}/proofs/390-open", draft.id),
            Some(&bruno),
            None,
        )
        .await;
        assert_eq!(status(&missing), StatusCode::NOT_FOUND);
        let _ = state;
    }
}
