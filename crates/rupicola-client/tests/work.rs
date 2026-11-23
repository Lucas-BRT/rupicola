//! The problem and fix calls against a stub server: each must reach its own route with the
//! method and token the backend expects, and decode the answer.

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri, header},
    routing::any,
};
use rupicola_api::{
    DesignKind, DesignStatus, Document, DocumentEdit, FixStatus, NewComment, NewDesignEntry,
    NewFix, NewProblem, NewReview, NewStep, ProblemOrigin, ProblemPatch, ProblemStatus, Severity,
    SystemAction, SystemActionForm,
};
use rupicola_client::{Api, HttpApi};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const TOKEN: &str = "stub-token";
const PROJECT: &str = "demo";
const PROBLEM: &str = "problem-1";
const FIX: &str = "fix-1";
const TITLE: &str = "Low contrast";
const COMMENT: &str = "seen again";

type Seen = Arc<Mutex<Vec<String>>>;

fn problem() -> Value {
    json!({
        "id": PROBLEM, "project_id": PROJECT, "title": TITLE, "status": "open",
        "severity": "major", "origin": "user-reported", "route": "/", "viewport": "",
        "theme": "", "diagnosis": "", "proposed_solution": "", "rules": [], "evidence": [], "selections": [],
        "comments": [], "author": null, "created_at": 0, "updated_at": 0
    })
}

fn fix() -> Value {
    json!({
        "id": FIX, "project_id": PROJECT, "title": TITLE, "summary": "", "status": "approved",
        "problems": [PROBLEM], "tactics": [], "files": [], "comparisons": [], "selections": [], "steps": [],
        "reviews": [], "author": null, "created_at": 0, "updated_at": 0
    })
}

/// Records `METHOD path token` and answers with a body of the type the route returns.
async fn stub(
    State(seen): State<Seen>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let path = uri.path().to_owned();
    seen.lock()
        .unwrap()
        .push(format!("{method} {path} {token}"));
    let version = json!({
        "id": "v1", "parent": null, "author": "ada", "phase": "draft", "generation": 0,
        "document": Document::default(), "notes": [], "report": null, "ready": false
    });
    let body = if path.ends_with("/design-systems") && method == Method::GET {
        json!({"items": [{"id": "v1", "name": "Evidence review", "phase": "draft"}], "active": null})
    } else if path.contains("/design-systems")
        && !path.ends_with("/preview")
        && !path.contains("/proofs/")
    {
        version
    } else if path.ends_with("/design") {
        let entry = json!({
            "id": "design-1", "kind": "tactic", "status": "adopted", "title": TITLE,
            "note": "", "surface": null, "supersedes": null, "reference": null,
            "evidence": null, "author": null, "created_at": 0
        });
        if method == Method::GET {
            json!([entry])
        } else {
            entry
        }
    } else if path.ends_with("/problems") && method == Method::GET {
        json!([{
            "id": PROBLEM, "title": TITLE, "status": "open", "severity": "major",
            "origin": "user-reported", "route": "/", "updated_at": 0
        }])
    } else if path.ends_with("/fixes") && method == Method::GET {
        json!([{
            "id": FIX, "title": TITLE, "status": "approved", "problems": 1,
            "updated_at": 0
        }])
    } else if path.ends_with("/comments") {
        json!({"id": "comment-1", "author": null, "text": COMMENT, "created_at": 0})
    } else if path.ends_with("/steps") {
        json!({"title": TITLE, "detail": "", "verification": "", "evidence": null, "created_at": 0})
    } else if path.contains("/fixes/") || path.ends_with("/fixes") {
        fix()
    } else {
        problem()
    };
    (StatusCode::OK, Json(body))
}

async fn serve() -> (HttpApi, Seen) {
    let seen = Seen::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .fallback(any(stub))
        .with_state(Arc::clone(&seen));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (HttpApi::new(&format!("http://{address}")).unwrap(), seen)
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one list of every call and path keeps the routes in view"
)]
async fn work_calls_reach_their_routes() {
    let (api, seen) = serve().await;
    assert_eq!(api.problems(TOKEN, PROJECT).await.unwrap()[0].id, PROBLEM);
    assert_eq!(api.problem(TOKEN, PROBLEM).await.unwrap().title, TITLE);
    let form = NewProblem {
        title: TITLE.to_owned(),
        severity: Severity::Major,
        origin: ProblemOrigin::UserReported,
        route: String::new(),
        viewport: String::new(),
        theme: String::new(),
        diagnosis: String::new(),
        proposed_solution: String::new(),
        rules: vec![],
        evidence: vec![],
        selections: Vec::new(),
    };
    api.create_problem(TOKEN, PROJECT, &form).await.unwrap();
    let patch = ProblemPatch {
        status: Some(ProblemStatus::Resolved),
        ..ProblemPatch::default()
    };
    api.update_problem(TOKEN, PROBLEM, &patch).await.unwrap();
    let comment = NewComment {
        text: COMMENT.to_owned(),
    };
    assert_eq!(
        api.comment_problem(TOKEN, PROBLEM, &comment)
            .await
            .unwrap()
            .text,
        COMMENT
    );
    assert_eq!(api.fixes(TOKEN, PROJECT).await.unwrap()[0].id, FIX);
    assert_eq!(api.fix(TOKEN, FIX).await.unwrap().id, FIX);
    let new_fix = NewFix {
        title: TITLE.to_owned(),
        summary: String::new(),
        problems: vec![PROBLEM.to_owned()],
        tactics: vec![],
        files: vec![],
        comparisons: vec![],
        steps: vec![],
        baseline: vec![],
        proof: vec![],
    };
    api.create_fix(TOKEN, PROJECT, &new_fix).await.unwrap();
    let step = NewStep {
        title: TITLE.to_owned(),
        detail: String::new(),
        verification: String::new(),
        evidence: None,
    };
    api.add_step(TOKEN, FIX, &step).await.unwrap();
    let review = NewReview {
        outcome: FixStatus::Approved,
        comment: String::new(),
    };
    api.review_fix(TOKEN, FIX, &review).await.unwrap();
    assert_eq!(api.design(TOKEN, PROJECT).await.unwrap()[0].id, "design-1");
    let entry = NewDesignEntry {
        kind: DesignKind::Tactic,
        status: DesignStatus::Adopted,
        title: TITLE.to_owned(),
        note: String::new(),
        surface: None,
        supersedes: None,
        reference: None,
        evidence: None,
    };
    api.add_design(TOKEN, PROJECT, &entry).await.unwrap();
    assert_eq!(
        api.design_systems(TOKEN, PROJECT).await.unwrap().items[0].id,
        "v1"
    );
    api.design_system(TOKEN, PROJECT, "v1").await.unwrap();
    api.create_design_system(TOKEN, PROJECT).await.unwrap();
    let edit = DocumentEdit {
        generation: 0,
        document: Document::default(),
    };
    api.edit_design_system(TOKEN, PROJECT, "v1", &edit)
        .await
        .unwrap();
    let form = SystemActionForm {
        action: SystemAction::Validate,
        generation: 0,
        target: String::new(),
        note: String::new(),
    };
    api.act_on_design_system(TOKEN, PROJECT, "v1", &form)
        .await
        .unwrap();
    api.design_preview(TOKEN, PROJECT, "v1").await.unwrap();
    api.design_proof(TOKEN, PROJECT, "v1", "390-open")
        .await
        .unwrap();
    let bearer = format!("Bearer {TOKEN}");
    let expected = [
        format!("GET /v1/projects/{PROJECT}/problems {bearer}"),
        format!("GET /v1/problems/{PROBLEM} {bearer}"),
        format!("POST /v1/projects/{PROJECT}/problems {bearer}"),
        format!("PATCH /v1/problems/{PROBLEM} {bearer}"),
        format!("POST /v1/problems/{PROBLEM}/comments {bearer}"),
        format!("GET /v1/projects/{PROJECT}/fixes {bearer}"),
        format!("GET /v1/fixes/{FIX} {bearer}"),
        format!("POST /v1/projects/{PROJECT}/fixes {bearer}"),
        format!("POST /v1/fixes/{FIX}/steps {bearer}"),
        format!("POST /v1/fixes/{FIX}/reviews {bearer}"),
        format!("GET /v1/projects/{PROJECT}/design {bearer}"),
        format!("POST /v1/projects/{PROJECT}/design {bearer}"),
        format!("GET /v1/projects/{PROJECT}/design-systems {bearer}"),
        format!("GET /v1/projects/{PROJECT}/design-systems/v1 {bearer}"),
        format!("POST /v1/projects/{PROJECT}/design-systems {bearer}"),
        format!("PUT /v1/projects/{PROJECT}/design-systems/v1/document {bearer}"),
        format!("POST /v1/projects/{PROJECT}/design-systems/v1/actions {bearer}"),
        format!("GET /v1/projects/{PROJECT}/design-systems/v1/preview {bearer}"),
        format!("GET /v1/projects/{PROJECT}/design-systems/v1/proofs/390-open {bearer}"),
    ];
    assert_eq!(*seen.lock().unwrap(), expected);
}
