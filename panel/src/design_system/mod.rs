mod view;

use crate::{
    Account, BoxError, PageResult, Panel, Response, StatusCode, auth, db_error, redirect, safe_name,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::Arc,
};
use topcoat::router::Body;
use turso::{Connection, transaction::TransactionBehavior};

pub use view::render;

const SCREEN: &str = include_str!("screen.html");
const VALIDATOR: &str = include_str!("validator.cjs");
const PREVIEW_POLICY: &str = "sandbox allow-scripts; default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; media-src data:; frame-ancestors 'self'; base-uri 'none'; form-action 'none'";

#[derive(Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum HeadingStyle {
    #[default]
    Editorial,
    Product,
}

impl HeadingStyle {
    const fn label(self) -> &'static str {
        match self {
            Self::Editorial => "Editorial serif",
            Self::Product => "Product sans",
        }
    }

    const fn font(self) -> &'static str {
        match self {
            Self::Editorial => "Georgia,serif",
            Self::Product => "system-ui,sans-serif",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct Guidance {
    button: String,
    form: String,
    table: String,
    comparison: String,
}

impl Default for Guidance {
    fn default() -> Self {
        Self {
            button: "Name the action. Keep one primary action per form. Show keyboard focus and disable repeat submission.".into(),
            form: "Label each field. Require a decision, and show the result beside the action.".into(),
            table: "Expose row and column headers. Keep horizontal scroll inside the table.".into(),
            comparison: "Use matching views for before and after. Offer side by side, overlay and clips. Restore focus when media returns.".into(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
struct Document {
    name: String,
    brand: String,
    landing_title: String,
    queue_title: String,
    queue_item: String,
    settings_title: String,
    direction: String,
    usage: String,
    screen_title: String,
    scenario: String,
    before_label: String,
    after_label: String,
    canvas: String,
    surface: String,
    ink: String,
    accent: String,
    action_ink: String,
    success: String,
    error: String,
    spacing: u8,
    radius: u8,
    heading: HeadingStyle,
    guidance: Guidance,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            name: "Evidence review".into(),
            brand: "Rupicola".into(),
            landing_title: "Interfaces under observation.".into(),
            queue_title: "Review captures".into(),
            queue_item: "Compact notification settings".into(),
            settings_title: "Workspace settings".into(),
            direction: "Warm paper, dark plumage and crest orange. Keep the evidence in focus.".into(),
            usage: "Use orange for the primary action. Keep labels short and use words alongside status colors.".into(),
            screen_title: "Compare evidence".into(),
            scenario: "Settings · Compact window · Example data".into(),
            before_label: "Before".into(),
            after_label: "After".into(),
            canvas: "#f7f1e7".into(),
            surface: "#fffaf2".into(),
            ink: "#231d19".into(),
            accent: "#ff681e".into(),
            action_ink: "#231d19".into(),
            success: "#40543c".into(),
            error: "#9c3617".into(),
            spacing: 16,
            radius: 6,
            heading: HeadingStyle::default(),
            guidance: Guidance::default(),
        }
    }
}

impl Document {
    fn valid(&self) -> bool {
        [
            (&self.name, 120),
            (&self.brand, 120),
            (&self.landing_title, 120),
            (&self.queue_title, 120),
            (&self.queue_item, 120),
            (&self.settings_title, 120),
            (&self.direction, 2500),
            (&self.usage, 2500),
            (&self.screen_title, 120),
            (&self.scenario, 200),
            (&self.before_label, 48),
            (&self.after_label, 48),
            (&self.guidance.button, 1400),
            (&self.guidance.form, 1400),
            (&self.guidance.table, 1400),
            (&self.guidance.comparison, 1400),
        ]
        .into_iter()
        .all(|(value, limit)| !value.trim().is_empty() && value.len() <= limit)
            && (8..=32).contains(&self.spacing)
            && self.radius <= 16
            && [
                &self.canvas,
                &self.surface,
                &self.ink,
                &self.accent,
                &self.action_ink,
                &self.success,
                &self.error,
            ]
            .into_iter()
            .all(|value| color(value).is_some())
    }

    fn contrast(&self) -> Result<(), &'static str> {
        for (front, back, error) in [
            (
                &self.ink,
                &self.canvas,
                "Text on canvas needs contrast of at least 4.5:1.",
            ),
            (
                &self.ink,
                &self.surface,
                "Text on surfaces needs contrast of at least 4.5:1.",
            ),
            (
                &self.action_ink,
                &self.accent,
                "Primary action text needs contrast of at least 4.5:1.",
            ),
            (
                &self.success,
                &self.surface,
                "Success messages need contrast of at least 4.5:1.",
            ),
            (
                &self.error,
                &self.surface,
                "Error messages need contrast of at least 4.5:1.",
            ),
        ] {
            let a = color(front).ok_or("Invalid foreground color.")?;
            let b = color(back).ok_or("Invalid background color.")?;
            if rupicola::visual::contrast(a, b) < 4.5 {
                return Err(error);
            }
        }
        Ok(())
    }
}

fn color(text: &str) -> Option<[u8; 3]> {
    (text.len() == 7
        && text.starts_with('#')
        && text.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))
    .then(|| rupicola::visual::parse_hex(text))
    .flatten()
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn image(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

fn render_screen(doc: &Document) -> String {
    [
        (
            "@@EXAMPLES_HTML@@",
            include_str!("examples.html").to_owned(),
        ),
        ("@@EXAMPLES_CSS@@", include_str!("examples.css").to_owned()),
        ("@@EXAMPLES_JS@@", include_str!("examples.js").to_owned()),
        ("@@CANVAS@@", doc.canvas.clone()),
        ("@@SURFACE@@", doc.surface.clone()),
        ("@@INK@@", doc.ink.clone()),
        ("@@ACCENT@@", doc.accent.clone()),
        ("@@ACTION_INK@@", doc.action_ink.clone()),
        ("@@SPACE@@", doc.spacing.to_string()),
        ("@@RADIUS@@", doc.radius.to_string()),
        ("@@HEADING@@", doc.heading.font().to_owned()),
        ("@@HEADING_LABEL@@", doc.heading.label().to_owned()),
        ("@@SUCCESS@@", doc.success.clone()),
        ("@@ERROR@@", doc.error.clone()),
        (
            "@@BEFORE_PNG@@",
            image("image/png", include_bytes!("fixtures/before.png")),
        ),
        (
            "@@AFTER_PNG@@",
            image("image/png", include_bytes!("fixtures/after.png")),
        ),
        (
            "@@BEFORE_WEBM@@",
            image("video/webm", include_bytes!("fixtures/before.webm")),
        ),
        (
            "@@AFTER_WEBM@@",
            image("video/webm", include_bytes!("fixtures/after.webm")),
        ),
        ("@@SYSTEM_NAME@@", escaped(&doc.name)),
        ("@@BRAND_ATTR@@", escaped(&doc.brand)),
        ("@@BRAND@@", escaped(&doc.brand)),
        ("@@LANDING_TITLE@@", escaped(&doc.landing_title)),
        ("@@QUEUE_TITLE@@", escaped(&doc.queue_title)),
        ("@@QUEUE_ITEM@@", escaped(&doc.queue_item)),
        ("@@SETTINGS_TITLE@@", escaped(&doc.settings_title)),
        ("@@BUTTON_NOTES@@", escaped(&doc.guidance.button)),
        ("@@FORM_NOTES@@", escaped(&doc.guidance.form)),
        ("@@TABLE_NOTES@@", escaped(&doc.guidance.table)),
        ("@@COMPARISON_NOTES@@", escaped(&doc.guidance.comparison)),
        ("@@TITLE@@", escaped(&doc.screen_title)),
        ("@@SCENARIO@@", escaped(&doc.scenario)),
        ("@@BEFORE_LABEL@@", escaped(&doc.before_label)),
        ("@@AFTER_LABEL@@", escaped(&doc.after_label)),
    ]
    .into_iter()
    .fold(SCREEN.to_owned(), |html, (key, value)| {
        html.replace(key, &value)
    })
}

fn fingerprint(doc: &Document) -> String {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(doc).expect("document contains only serializable fields"));
    hash.update(render_screen(doc).as_bytes());
    hash.update(VALIDATOR.as_bytes());
    format!("{:x}", hash.finalize())
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Phase {
    Draft,
    Review,
    Approved,
    Changes,
}

impl Phase {
    const fn key(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Review => "review",
            Self::Approved => "approved",
            Self::Changes => "changes",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Review => "In review",
            Self::Approved => "Approved",
            Self::Changes => "Changes requested",
        }
    }
}

#[derive(Deserialize, Serialize)]
struct Note {
    actor: String,
    time: i64,
    target: String,
    action: String,
    text: String,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Outcome {
    Passed,
    Failed,
    Blocked,
}

#[derive(Deserialize, Serialize)]
struct Shot {
    name: String,
    png: String,
}

#[derive(Deserialize, Serialize)]
struct Report {
    #[serde(default)]
    fingerprint: String,
    baseline: Option<String>,
    outcome: Outcome,
    browser: Option<String>,
    checks: Vec<String>,
    shots: Vec<Shot>,
    error: Option<String>,
}

impl Report {
    fn error(outcome: Outcome, error: impl Into<String>) -> Self {
        Self {
            fingerprint: String::new(),
            baseline: None,
            outcome,
            browser: None,
            checks: Vec::new(),
            shots: Vec::new(),
            error: Some(error.into()),
        }
    }

    fn passed(&self) -> bool {
        self.outcome == Outcome::Passed
    }
}

#[derive(Deserialize, Serialize)]
pub struct Version {
    #[serde(skip)]
    id: String,
    #[serde(skip)]
    generation: i64,
    parent: Option<String>,
    author: String,
    phase: Phase,
    document: Document,
    notes: Vec<Note>,
    report: Option<Report>,
    frozen_screen: Option<String>,
}

impl Version {
    fn draft(id: String, parent: Option<String>, author: String, document: Document) -> Self {
        Self {
            id,
            generation: 0,
            parent,
            author,
            phase: Phase::Draft,
            document,
            notes: vec![],
            report: None,
            frozen_screen: None,
        }
    }

    fn ready(&self, adopted: Option<&str>) -> bool {
        self.report.as_ref().is_some_and(|report| {
            report.passed()
                && report.baseline.as_deref() == adopted
                && report.fingerprint == fingerprint(&self.document)
        })
    }

    fn submit(&mut self, adopted: Option<&str>) -> PageResult<()> {
        if self.phase != Phase::Draft || !self.ready(adopted) {
            return Err(StatusCode::CONFLICT);
        }
        self.frozen_screen = Some(render_screen(&self.document));
        self.phase = Phase::Review;
        Ok(())
    }

    fn note(&mut self, actor: &str, action: &str, target: &str, text: String) {
        self.notes.push(Note {
            actor: actor.to_owned(),
            time: auth::now(),
            action: action.to_owned(),
            target: target.to_owned(),
            text,
        });
    }
}

pub async fn init(db: &Connection) -> Result<(), BoxError> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS design_versions_v1 (
          project TEXT NOT NULL, id TEXT NOT NULL, generation INTEGER NOT NULL,
          created INTEGER NOT NULL, name TEXT NOT NULL, phase TEXT NOT NULL,
          payload TEXT NOT NULL, PRIMARY KEY(project, id)
        );
        CREATE TABLE IF NOT EXISTS design_adoption_v1 (
          project TEXT PRIMARY KEY, version TEXT NOT NULL
        );",
    )
    .await?;
    Ok(())
}

pub struct Studio {
    items: Vec<(String, String, Phase)>,
    selected: Option<Version>,
    active: Option<String>,
}

pub async fn load(panel: &Panel, project: &str, selected: Option<&str>) -> PageResult<Studio> {
    let db = panel.index.lock().await;
    let active = active_version(&db, project).await?;
    let mut records = db
        .query(
            "SELECT id, name, phase
         FROM design_versions_v1 WHERE project=? ORDER BY created DESC, rowid DESC LIMIT 100",
            [project],
        )
        .await
        .map_err(db_error)?;
    let mut items: Vec<(String, String, Phase)> = Vec::new();
    while let Some(row) = records.next().await.map_err(db_error)? {
        let phase: String = row.get(2).map_err(db_error)?;
        let phase = serde_json::from_value(serde_json::Value::String(phase)).map_err(db_error)?;
        items.push((
            row.get(0).map_err(db_error)?,
            row.get(1).map_err(db_error)?,
            phase,
        ));
    }
    drop(records);
    let selected = selected
        .or(active.as_deref())
        .or_else(|| items.first().map(|item| item.0.as_str()));
    let selected = match selected {
        Some(id) => Some(read(&db, project, id).await?),
        None => None,
    };
    Ok(Studio {
        items,
        selected,
        active,
    })
}

async fn active_version(db: &Connection, project: &str) -> PageResult<Option<String>> {
    let mut records = db
        .query(
            "SELECT version FROM design_adoption_v1 WHERE project=?",
            [project],
        )
        .await
        .map_err(db_error)?;
    records
        .next()
        .await
        .map_err(db_error)?
        .map(|row| row.get(0))
        .transpose()
        .map_err(db_error)
}

async fn read(db: &Connection, project: &str, id: &str) -> PageResult<Version> {
    let mut rows = db
        .query(
            "SELECT generation, payload FROM design_versions_v1 WHERE project=? AND id=?",
            [project, id],
        )
        .await
        .map_err(db_error)?;
    let row = rows
        .next()
        .await
        .map_err(db_error)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let data: String = row.get(1).map_err(db_error)?;
    let mut version: Version = serde_json::from_str(&data).map_err(db_error)?;
    id.clone_into(&mut version.id);
    version.generation = row.get(0).map_err(db_error)?;
    Ok(version)
}

async fn insert(db: &Connection, project: &str, version: &Version) -> PageResult<()> {
    db.execute(
        "INSERT INTO design_versions_v1 (project, id, generation, created, name, phase, payload)
         VALUES (?, ?, 0, ?, ?, ?, ?)",
        turso::params![
            project,
            version.id.clone(),
            auth::now(),
            version.document.name.as_str(),
            version.phase.key(),
            serde_json::to_string(version).map_err(db_error)?
        ],
    )
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn save(db: &Connection, project: &str, version: &Version) -> PageResult<()> {
    let rows = db
        .execute(
            "UPDATE design_versions_v1 SET generation=generation+1, name=?, phase=?, payload=?
         WHERE project=? AND id=? AND generation=?",
            turso::params![
                version.document.name.as_str(),
                version.phase.key(),
                serde_json::to_string(version).map_err(db_error)?,
                project,
                version.id.clone(),
                version.generation
            ],
        )
        .await
        .map_err(db_error)?;
    if rows == 1 {
        Ok(())
    } else {
        Err(StatusCode::CONFLICT)
    }
}

#[derive(Deserialize)]
pub struct EditForm {
    csrf: String,
    version: String,
    generation: i64,
    name: String,
    brand: String,
    landing_title: String,
    queue_title: String,
    queue_item: String,
    settings_title: String,
    direction: String,
    usage: String,
    screen_title: String,
    scenario: String,
    before_label: String,
    after_label: String,
    canvas: String,
    surface: String,
    ink: String,
    accent: String,
    action_ink: String,
    success: String,
    error: String,
    spacing: u8,
    radius: u8,
    heading: HeadingStyle,
    button_notes: String,
    form_notes: String,
    table_notes: String,
    comparison_notes: String,
}

pub async fn edit(
    panel: &Panel,
    account: &Account,
    project: &str,
    form: EditForm,
) -> PageResult<Response> {
    owner(account, project, &form.csrf)?;
    let document = Document {
        name: form.name,
        brand: form.brand,
        landing_title: form.landing_title,
        queue_title: form.queue_title,
        queue_item: form.queue_item,
        settings_title: form.settings_title,
        direction: form.direction,
        usage: form.usage,
        screen_title: form.screen_title,
        scenario: form.scenario,
        before_label: form.before_label,
        after_label: form.after_label,
        canvas: form.canvas,
        surface: form.surface,
        ink: form.ink,
        accent: form.accent,
        action_ink: form.action_ink,
        success: form.success,
        error: form.error,
        spacing: form.spacing,
        radius: form.radius,
        heading: form.heading,
        guidance: Guidance {
            button: form.button_notes,
            form: form.form_notes,
            table: form.table_notes,
            comparison: form.comparison_notes,
        },
    };
    patch_document(panel, project, &form.version, form.generation, document).await
}

async fn patch_document(
    panel: &Panel,
    project: &str,
    id: &str,
    generation: i64,
    document: Document,
) -> PageResult<Response> {
    if !document.valid() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let db = panel.index.lock().await;
    let mut version = read(&db, project, id).await?;
    if version.phase != Phase::Draft || version.generation != generation {
        return Err(StatusCode::CONFLICT);
    }
    version.document = document;
    version.report = None;
    save(&db, project, &version).await?;
    Ok(location(project, "system", &version.id))
}

#[derive(Deserialize)]
pub struct ActionForm {
    #[serde(default)]
    csrf: String,
    action: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    generation: i64,
    #[serde(default)]
    note: String,
    #[serde(default)]
    target: String,
}

fn owner(account: &Account, project: &str, csrf: &str) -> PageResult<()> {
    auth::check_csrf(account, csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    if !safe_name(project) {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(())
}

fn location(project: &str, view: &str, id: &str) -> Response {
    redirect(&format!(
        "/projects/{project}/design?view={view}&version={id}"
    ))
}

pub async fn action(
    panel: Arc<Panel>,
    account: Account,
    project: &str,
    form: &ActionForm,
) -> PageResult<Response> {
    owner(&account, project, &form.csrf)?;
    apply_action(&panel, &account.handle, project, form).await
}

async fn apply_action(
    panel: &Panel,
    actor: &str,
    project: &str,
    form: &ActionForm,
) -> PageResult<Response> {
    let db = panel.index.lock().await;
    if form.action == "create" {
        let parent = active_version(&db, project).await?;
        let document = match parent.as_deref() {
            Some(id) => read(&db, project, id).await?.document,
            None => Document::default(),
        };
        let new = Version::draft(auth::token(), parent, actor.to_owned(), document);
        insert(&db, project, &new).await?;
        return Ok(location(project, "system", &new.id));
    }
    let mut version = read(&db, project, &form.version).await?;
    if version.generation != form.generation {
        return Err(StatusCode::CONFLICT);
    }
    if form.action == "validate" {
        if version.phase != Phase::Draft {
            return Err(StatusCode::CONFLICT);
        }
        let active = active_version(&db, project).await?;
        let baseline = if let Some(id) = active.as_deref() {
            let mut doc = read(&db, project, id).await?.document;
            doc.name.clone_from(&version.document.name);
            doc.brand.clone_from(&version.document.brand);
            doc.landing_title
                .clone_from(&version.document.landing_title);
            doc.queue_title.clone_from(&version.document.queue_title);
            doc.queue_item.clone_from(&version.document.queue_item);
            doc.settings_title
                .clone_from(&version.document.settings_title);
            doc.screen_title.clone_from(&version.document.screen_title);
            doc.scenario.clone_from(&version.document.scenario);
            doc.before_label.clone_from(&version.document.before_label);
            doc.after_label.clone_from(&version.document.after_label);
            Some(render_screen(&doc))
        } else {
            None
        };
        drop(db);
        let _busy = panel
            .design_validation
            .try_acquire()
            .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
        let document = version.document.clone();
        let mut report =
            tokio::task::spawn_blocking(move || validate(&document, baseline.as_deref()))
                .await
                .map_err(db_error)?;
        let db = panel.index.lock().await;
        if active_version(&db, project).await? != active {
            return Err(StatusCode::CONFLICT);
        }
        report.baseline = active;
        version.report = Some(report);
        save(&db, project, &version).await?;
        return Ok(location(project, "screens", &version.id));
    }
    if form.action == "submit" {
        let active = active_version(&db, project).await?;
        version.submit(active.as_deref())?;
        save(&db, project, &version).await?;
        return Ok(location(project, "proposals", &version.id));
    }
    if form.action == "variation" {
        let new = Version::draft(
            auth::token(),
            Some(version.id),
            actor.to_owned(),
            version.document,
        );
        insert(&db, project, &new).await?;
        return Ok(location(project, "system", &new.id));
    }
    if form.action == "adopt" {
        adopt(&db, project, &mut version, actor).await?;
        return Ok(location(project, "system", &version.id));
    }
    review(&mut version, actor, form)?;
    save(&db, project, &version).await?;
    Ok(location(project, "proposals", &version.id))
}

fn review(version: &mut Version, actor: &str, form: &ActionForm) -> PageResult<()> {
    if form.note.len() > 4000 || version.notes.len() >= 1000 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let text = form.note.trim();
    let target = match form.target.as_str() {
        "identity" | "tokens" | "components" | "comparison" | "media-unavailable" | "form" => {
            &form.target
        }
        _ => "document",
    };
    match form.action.as_str() {
        "comment" if version.phase != Phase::Draft && !text.is_empty() => (),
        "approve" if version.phase == Phase::Review => version.phase = Phase::Approved,
        "changes" if version.phase == Phase::Review && !text.is_empty() => {
            version.phase = Phase::Changes;
        }
        _ => return Err(StatusCode::CONFLICT),
    }
    version.note(actor, &form.action, target, text.to_owned());
    Ok(())
}

async fn adopt(
    db: &Connection,
    project: &str,
    version: &mut Version,
    actor: &str,
) -> PageResult<()> {
    if version.phase != Phase::Approved || version.frozen_screen.is_none() {
        return Err(StatusCode::CONFLICT);
    }
    if active_version(db, project).await?.as_deref() == Some(&version.id) {
        return Ok(());
    }
    if version.notes.len() >= 1000 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    version.note(actor, "adopt", "document", String::new());
    db.transactional_batch(
        [
            ("UPDATE design_versions_v1 SET generation=generation+1,payload=? WHERE project=? AND id=? AND generation=?",
             Vec::from(turso::params![serde_json::to_string(version).map_err(db_error)?, project, version.id.clone(), version.generation])),
            ("INSERT INTO design_adoption_v1 VALUES (?, ?) ON CONFLICT(project) DO UPDATE SET version=excluded.version",
             Vec::from(turso::params![project, version.id.clone()])),
        ],
        TransactionBehavior::Immediate,
    ).await.map_err(db_error)?;
    Ok(())
}

fn validate(doc: &Document, baseline: Option<&str>) -> Report {
    let mut result = if !doc.valid() {
        Report::error(
            Outcome::Failed,
            "The design document is incomplete or contains invalid tokens.",
        )
    } else if let Err(error) = doc.contrast() {
        Report::error(Outcome::Failed, error)
    } else {
        run_browser(&render_screen(doc), baseline)
            .unwrap_or_else(|error| Report::error(Outcome::Blocked, error))
    };
    result.fingerprint = fingerprint(doc);
    result
}

fn run_browser(html: &str, baseline: Option<&str>) -> Result<Report, String> {
    let input = serde_json::json!({ "html": html, "baseline": baseline }).to_string();
    let mut command = Command::new("node");
    command.env_clear();
    for key in [
        "PATH",
        "HOME",
        "TMPDIR",
        "PLAYWRIGHT_BROWSERS_PATH",
        "RUPICOLA_PLAYWRIGHT_MODULE",
        "RUPICOLA_CHROMIUM",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = command
        .args(["-e", VALIDATOR])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Node.js is unavailable on the panel host.".to_owned())?;
    let sent = child
        .stdin
        .take()
        .ok_or("Cannot open validator input.")?
        .write_all(input.as_bytes());
    let output = child
        .wait_with_output()
        .map_err(|_| "Browser validator did not finish.")?;
    sent.map_err(|_| "Cannot send the preview to the validator.")?;
    if !output.status.success() || output.stdout.len() > 18 * 1024 * 1024 {
        return Err("Browser validator timed out or returned an invalid report.".into());
    }
    let report: Report =
        serde_json::from_slice(&output.stdout).map_err(|_| "Invalid browser validation report.")?;
    if report.passed()
        && (report.checks.len() != if baseline.is_some() { 18 } else { 16 }
            || report.shots.len() != if baseline.is_some() { 24 } else { 12 }
            || report.browser.is_none())
    {
        return Err("Browser validator omitted mandatory evidence.".into());
    }
    Ok(report)
}

pub async fn preview(
    panel: &Panel,
    account: &Account,
    project: &str,
    id: &str,
) -> PageResult<Response> {
    auth::role(panel, account, project).await?;
    let html = preview_html(panel, project, id).await?;
    Response::builder()
        .header(http::header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(http::header::CONTENT_SECURITY_POLICY, PREVIEW_POLICY)
        .body(Body::from(html))
        .map_err(db_error)
}

pub(crate) async fn preview_html(panel: &Panel, project: &str, id: &str) -> PageResult<String> {
    let db = panel.index.lock().await;
    let version = read(&db, project, id).await?;
    Ok(version
        .frozen_screen
        .unwrap_or_else(|| render_screen(&version.document)))
}

pub async fn proof(
    panel: &Panel,
    account: &Account,
    project: &str,
    id: &str,
    name: &str,
) -> PageResult<Response> {
    auth::role(panel, account, project).await?;
    let png = STANDARD
        .decode(proof_base64(panel, project, id, name).await?)
        .map_err(db_error)?;
    Response::builder()
        .header(http::header::CONTENT_TYPE, "image/png")
        .body(Body::from(png))
        .map_err(db_error)
}

pub(crate) async fn proof_base64(
    panel: &Panel,
    project: &str,
    id: &str,
    name: &str,
) -> PageResult<String> {
    let db = panel.index.lock().await;
    let version = read(&db, project, id).await?;
    version
        .report
        .and_then(|report| report.shots.into_iter().find(|shot| shot.name == name))
        .map(|shot| shot.png)
        .ok_or(StatusCode::NOT_FOUND)
}

pub(crate) async fn mcp_action(
    panel: &Panel,
    actor: &str,
    project: &str,
    args: &serde_json::Value,
) -> PageResult<Response> {
    let form = ActionForm::deserialize(args).map_err(|_| StatusCode::BAD_REQUEST)?;
    if !matches!(
        form.action.as_str(),
        "create"
            | "variation"
            | "validate"
            | "submit"
            | "comment"
            | "approve"
            | "changes"
            | "adopt"
    ) || (form.action != "create"
        && (form.version.is_empty() || args.get("generation").is_none()))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    apply_action(panel, actor, project, &form).await
}

pub(crate) async fn mcp_edit(
    panel: &Panel,
    project: &str,
    id: &str,
    generation: i64,
    changes: &serde_json::Value,
) -> PageResult<Response> {
    let version = {
        let db = panel.index.lock().await;
        read(&db, project, id).await?
    };
    let mut document = serde_json::to_value(version.document).map_err(db_error)?;
    let changes = changes
        .as_object()
        .filter(|map| !map.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let map = document
        .as_object_mut()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    for (key, value) in changes {
        let current = map.get_mut(key).ok_or(StatusCode::BAD_REQUEST)?;
        if key == "guidance" {
            let guidance = current
                .as_object_mut()
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            for (field, value) in value.as_object().ok_or(StatusCode::BAD_REQUEST)? {
                *guidance.get_mut(field).ok_or(StatusCode::BAD_REQUEST)? = value.clone();
            }
        } else {
            *current = value.clone();
        }
    }
    let doc = serde_json::from_value(document).map_err(|_| StatusCode::BAD_REQUEST)?;
    patch_document(panel, project, id, generation, doc).await
}

pub async fn mcp_read(
    panel: &Panel,
    project: &str,
    id: Option<&str>,
) -> PageResult<serde_json::Value> {
    use serde_json::json;
    if let Some(id) = id {
        let db = panel.index.lock().await;
        let version = read(&db, project, id).await?;
        return Ok(
            json!({"id": id, "phase": version.phase, "generation": version.generation,
            "parent": version.parent, "document": version.document, "notes": version.notes,
            "adopted": active_version(&db, project).await?.as_deref() == Some(id),
            "validation": version.report.map(|report| json!({"outcome": report.outcome,
                "checks": report.checks, "error": report.error, "baseline": report.baseline,
                "shots": report.shots.into_iter().map(|shot| shot.name).collect::<Vec<_>>() }))}),
        );
    }
    let studio = load(panel, project, None).await?;
    Ok(
        json!({"adopted": studio.active, "versions": studio.items.into_iter()
        .map(|(id, name, phase)| json!({"id": id, "name": name, "phase": phase})).collect::<Vec<_>>() }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passing_report(document: &Document) -> Report {
        let mut report = Report::error(Outcome::Passed, "");
        report.fingerprint = fingerprint(document);
        report
    }

    #[test]
    fn contrast_failure_cannot_submit() {
        let document = Document {
            action_ink: "#ff681e".into(),
            ..Document::default()
        };
        assert!(document.valid());
        let mut version = Version::draft("v1".into(), None, "owner".into(), document);
        version.report = Some(validate(&version.document, None));
        assert!(
            version
                .report
                .as_ref()
                .is_some_and(|report| report.outcome == Outcome::Failed)
        );
        assert_eq!(version.submit(None), Err(StatusCode::CONFLICT));
        assert!(version.frozen_screen.is_none());
    }

    #[test]
    fn editing_a_token_invalidates_proof_and_submission_freezes_screen() {
        let mut version = Version::draft("v1".into(), None, "owner".into(), Document::default());
        assert_eq!(version.submit(None), Err(StatusCode::CONFLICT));
        version.report = Some(passing_report(&version.document));
        assert!(version.ready(None));
        version.document.accent = "#f76e25".into();
        assert_eq!(version.submit(None), Err(StatusCode::CONFLICT));
        version.report = Some(passing_report(&version.document));
        version.submit(None).unwrap();
        assert!(version.phase == Phase::Review);
        assert!(
            version
                .frozen_screen
                .as_ref()
                .unwrap()
                .contains("--accent:#f76e25")
        );
        assert_eq!(version.submit(None), Err(StatusCode::CONFLICT));
    }

    #[test]
    fn screen_escapes_user_text_and_rejects_css_in_tokens() {
        let mut document = Document {
            screen_title: "</title><script>bad()</script>".into(),
            ..Document::default()
        };
        let rendered = render_screen(&document);
        assert!(rendered.contains("&lt;/title&gt;&lt;script&gt;bad()"));
        assert!(!rendered.contains("<script>bad()"));
        document.accent = "#f0f; };".into();
        assert!(!document.valid());
    }

    #[tokio::test]
    async fn stale_write_and_adoption_preserve_history() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("panel.db");
        let db = turso::Builder::new_local(db_path.to_str().unwrap())
            .build()
            .await
            .unwrap()
            .connect()
            .unwrap();
        init(&db).await.unwrap();
        let draft = Version::draft("v1".into(), None, "owner".into(), Document::default());
        insert(&db, "project", &draft).await.unwrap();
        assert!(matches!(
            read(&db, "other", "v1").await,
            Err(StatusCode::NOT_FOUND)
        ));
        let mut current = read(&db, "project", "v1").await.unwrap();
        assert_eq!(
            adopt(&db, "project", &mut current, "owner").await,
            Err(StatusCode::CONFLICT)
        );
        current.report = Some(passing_report(&current.document));
        current.submit(None).unwrap();
        save(&db, "project", &current).await.unwrap();
        assert_eq!(
            save(&db, "project", &draft).await,
            Err(StatusCode::CONFLICT)
        );
        let mut review = read(&db, "project", "v1").await.unwrap();
        review.phase = Phase::Approved;
        save(&db, "project", &review).await.unwrap();
        let mut approved = read(&db, "project", "v1").await.unwrap();
        adopt(&db, "project", &mut approved, "owner").await.unwrap();
        adopt(&db, "project", &mut approved, "owner").await.unwrap();
        assert_eq!(
            active_version(&db, "project").await.unwrap().as_deref(),
            Some("v1")
        );
        let adopted = read(&db, "project", "v1").await.unwrap();
        assert_eq!(adopted.notes.len(), 1);
        assert_eq!(adopted.notes[0].action, "adopt");
        assert!(adopted.frozen_screen.is_some());
    }
}
