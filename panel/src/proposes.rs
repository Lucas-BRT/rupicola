use crate::{
    PageResult, Panel, auth,
    auth::{Account, Role},
    db_error, design_record,
    proposal_record::{
        self, Attachment, AttachmentKind, Comment, Dispatch, Origin, Proposal, Status,
    },
    redirect, safe_name,
};
use http::{HeaderMap, StatusCode, header};
use std::fmt::Write as _;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use topcoat::{
    context::Cx,
    router::{Body, response::Response, to_bytes},
    view::{Child, view},
};

const MAX_UPLOAD_BYTES: usize = 24 * 1024 * 1024;
const MAX_SINGLE_FILE: usize = 16 * 1024 * 1024;

const SKILL_TACTICS: &[(&str, &str)] = &[
    ("visual-hierarchy", "Visual hierarchy"),
    ("spacing-rhythm", "Spacing & rhythm"),
    ("contrast-tokens", "Contrast & tokens"),
    ("focus-navigation", "Focus & keyboard"),
    ("motion-intent", "Motion with intent"),
    ("empty-error-states", "Empty & error states"),
    ("progressive-disclosure", "Progressive disclosure"),
    ("evidence-first", "Evidence-first layout"),
];

pub struct ProposeFilter {
    pub kind: Option<String>,
    pub status: Option<String>,
}

fn now_ms() -> Result<u64, StatusCode> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(db_error)?;
    u64::try_from(elapsed.as_millis()).map_err(db_error)
}

fn data_dir(panel: &Panel) -> &Path {
    panel.design.parent().unwrap_or(Path::new("."))
}

fn proposals_root(panel: &Panel, project_id: &str) -> PathBuf {
    data_dir(panel)
        .join(proposal_record::DIRECTORY)
        .join(project_id)
}

fn safe_asset(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn content_type_for(name: &str) -> Option<(&'static str, AttachmentKind)> {
    let (_, extension) = name.rsplit_once('.')?;
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some(("image/png", AttachmentKind::Image)),
        "jpg" | "jpeg" => Some(("image/jpeg", AttachmentKind::Image)),
        "webp" => Some(("image/webp", AttachmentKind::Image)),
        "mp4" => Some(("video/mp4", AttachmentKind::Video)),
        "webm" => Some(("video/webm", AttachmentKind::Video)),
        _ => None,
    }
}

fn valid_magic(ext_mime: &str, bytes: &[u8]) -> bool {
    match ext_mime {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/webp" => bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "video/mp4" => bytes.len() >= 8 && &bytes[4..8] == b"ftyp",
        "video/webm" => bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]),
        _ => false,
    }
}

struct MultipartPart {
    name: String,
    filename: Option<String>,
    data: Vec<u8>,
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_multipart(boundary: &str, body: &[u8]) -> Result<Vec<MultipartPart>, StatusCode> {
    let delim = format!("--{boundary}").into_bytes();
    let mut parts = Vec::new();
    let mut pos = find_bytes(body, &delim).ok_or(StatusCode::BAD_REQUEST)?;
    pos += delim.len();
    loop {
        if body[pos..].starts_with(b"--") {
            break;
        }
        if body[pos..].starts_with(b"\r\n") {
            pos += 2;
        } else if body[pos..].starts_with(b"\n") {
            pos += 1;
        } else {
            return Err(StatusCode::BAD_REQUEST);
        }
        let header_end = find_bytes(&body[pos..], b"\r\n\r\n").ok_or(StatusCode::BAD_REQUEST)?;
        let header_str = std::str::from_utf8(&body[pos..pos + header_end])
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        pos += header_end + 4;
        let next = find_bytes(&body[pos..], &delim).ok_or(StatusCode::BAD_REQUEST)?;
        let mut data_end = pos + next;
        if data_end >= 2 && &body[data_end - 2..data_end] == b"\r\n" {
            data_end -= 2;
        }
        let mut name = String::new();
        let mut filename = None;
        for line in header_str.lines() {
            if let Some(disp) = line
                .strip_prefix("Content-Disposition:")
                .or_else(|| line.strip_prefix("content-disposition:"))
            {
                for seg in disp.split(';') {
                    let seg = seg.trim();
                    if let Some(val) = seg.strip_prefix("name=\"") {
                        val.trim_end_matches('"').clone_into(&mut name);
                    } else if let Some(val) = seg.strip_prefix("filename=\"") {
                        let f = val.trim_end_matches('"');
                        if !f.is_empty() {
                            filename = Some(f.to_owned());
                        }
                    }
                }
            }
        }
        if !name.is_empty() {
            parts.push(MultipartPart {
                name,
                filename,
                data: body[pos..data_end].to_vec(),
            });
        }
        pos = pos + next + delim.len();
    }
    Ok(parts)
}

fn parse_kind(raw: &str) -> proposal_record::Kind {
    match raw {
        "palette" => proposal_record::Kind::Palette,
        "component" => proposal_record::Kind::Component,
        "page" => proposal_record::Kind::Page,
        "layout" => proposal_record::Kind::Layout,
        "application" => proposal_record::Kind::Application,
        "design-system" => proposal_record::Kind::DesignSystem,
        "motion" => proposal_record::Kind::Motion,
        _ => proposal_record::Kind::Other,
    }
}

pub struct ProposeState {
    pub filter: ProposeFilter,
    pub all: Vec<Proposal>,
    pub mcp: Option<proposal_record::McpSession>,
}

pub fn load(panel: &Panel, project_id: &str, filter: ProposeFilter) -> PageResult<ProposeState> {
    let root = proposals_root(panel, project_id);
    let all = proposal_record::entries(&root).map_err(db_error)?;
    let mcp = proposal_record::mcp_session_active(data_dir(panel));
    Ok(ProposeState { filter, all, mcp })
}

#[expect(
    clippy::too_many_lines,
    reason = "the proposals view keeps markup and presentation branches together"
)]
pub fn render_section<'a>(
    cx: &'a Cx,
    account: &'a Account,
    role: Role,
    project_id: &'a str,
    base: &'a str,
    state: &'a ProposeState,
) -> Child<'a> {
    let filter = &state.filter;
    let mcp = state.mcp.as_ref();
    let filtered: Vec<_> = state
        .all
        .iter()
        .filter(|p| {
            filter
                .kind
                .as_deref()
                .filter(|k| !k.is_empty())
                .is_none_or(|k| p.kind.slug() == k)
                && filter
                    .status
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .is_none_or(|s| p.status.slug() == s)
        })
        .collect();

    Child::new(view! {
        cx =>
        <div class="propose-toolbar">
            <div class="propose-mcp-state">
                if let Some(session) = &mcp {
                    <span class="status">"MCP connected · PID " (session.pid)</span>
                } else {
                    <span class="muted">"MCP offline · connect an agent to dispatch proposals"</span>
                }
            </div>
            <form class="filters" method="get" action=(format!("{base}/proposes"))>
                <label for="prop-kind">"Type"</label>
                <select id="prop-kind" name="kind">
                    <option value="">"All types"</option>
                    for k in proposal_record::Kind::ALL {
                        <option value=(k.slug()) if filter.kind.as_deref() == Some(k.slug()) { selected="" }>(k.label())</option>
                    }
                </select>
                <label for="prop-status">"Status"</label>
                <select id="prop-status" name="status">
                    <option value="">"All statuses"</option>
                    for (slug, label) in [
                        ("pending-extraction", "Pending extraction"),
                        ("proposed", "Proposed"),
                        ("approved", "Approved"),
                        ("rejected", "Rejected"),
                        ("variation-requested", "Variation requested"),
                    ] {
                        <option value=(slug) if filter.status.as_deref() == Some(slug) { selected="" }>(label)</option>
                    }
                </select>
                <button class="button" type="submit">"Filter"</button>
            </form>
        </div>

        if role == Role::Owner {
            <details class="surface panel-padding design-editor">
                <summary>"New proposal (with media, links, tactics & prompt)"</summary>
                <form class="stack-form propose-create-form" method="post" action=(format!("{base}/proposes/create")) enctype="multipart/form-data">
                    <input type="hidden" name="csrf" value=(&account.csrf)>
                    <label for="new-prop-kind">"Proposal type"</label>
                    <select id="new-prop-kind" name="kind">
                        for k in proposal_record::Kind::ALL {
                            <option value=(k.slug())>(k.label())</option>
                        }
                    </select>
                    <label for="new-prop-title">"Title"</label>
                    <input id="new-prop-title" name="title" required="" maxlength="200">
                    <label for="new-prop-problem">"Problem solved"</label>
                    <textarea id="new-prop-problem" name="problem_solved" required="" maxlength="2000" rows="2"></textarea>
                    <label for="new-prop-rationale">"Why this approach"</label>
                    <textarea id="new-prop-rationale" name="rationale" maxlength="4000" rows="3"></textarea>
                    <label for="new-prop-prompt">"Generation prompt / goal"</label>
                    <textarea id="new-prop-prompt" name="prompt" maxlength="4000" rows="3"></textarea>
                    <label for="new-prop-surface">"Surface / stack (optional)"</label>
                    <input id="new-prop-surface" name="surface" maxlength="120" placeholder="e.g. Web · Overview, or Iced desktop">
                    <fieldset class="tactics-fieldset">
                        <legend>"Skill tactics applied"</legend>
                        <div class="tactics-checks">
                            for (slug, label) in SKILL_TACTICS {
                                <label class="tactic-check"><input type="checkbox" name="tactics" value=(*slug)><span>(*label)</span></label>
                            }
                        </div>
                    </fieldset>
                    <label for="new-prop-refs">"Reference URLs (one per line, optional)"</label>
                    <textarea id="new-prop-refs" name="references" rows="2" maxlength="4000" placeholder="https://..."></textarea>
                    <label for="new-prop-files">"Reference images or videos (PNG, JPG, WebP, MP4, WebM)"</label>
                    <input id="new-prop-files" name="attachments" type="file" multiple="" accept="image/png,image/jpeg,image/webp,video/mp4,video/webm">
                    <button class="button primary" type="submit">"Create proposal"</button>
                </form>
            </details>
        }

        if filtered.is_empty() {
            <section class="empty">
                <h2>"No proposals match this filter"</h2>
                <p>"Create a proposal above or save one of the starter visual directions below."</p>
            </section>
        } else {
            <div class="propose-catalog">
                for p in &filtered {
                    <article class="propose-card" data-proposal-card=(&p.id)>
                        <header class="propose-head">
                            <div>
                                <div class="propose-badges">
                                    <span class="status">(p.kind.label())</span>
                                    <span class=(if matches!(p.status, Status::Rejected) { "status blocked" } else { "status" })>(p.status.label())</span>
                                    if p.parent_id.is_some() { <span class="muted">"Variation"</span> }
                                </div>
                                <h2>(&p.title)</h2>
                            </div>
                            <button class="button primary" type="button" data-proposal-open=(&p.id)>"Inspect"</button>
                        </header>
                        if !p.problem_solved.is_empty() {
                            <p class="muted">(&p.problem_solved)</p>
                        }
                        if let Some(ext) = &p.extracted {
                            if !ext.palette_hex.is_empty() {
                                <div class="swatch-row" aria-label="Extracted palette">
                                    for hex in &ext.palette_hex {
                                        <span class="swatch-chip"><code>(hex)</code></span>
                                    }
                                </div>
                            }
                        }
                        if !p.tactics.is_empty() {
                            <div class="tactic-chips">
                                for t in &p.tactics {
                                    <span class="tactic-chip">(t)</span>
                                }
                            </div>
                        }
                        <footer class="propose-foot">
                            <span class="muted">(p.comments.len()) " comments · " (p.attachments.len()) " files"</span>
                            if role == Role::Owner {
                                if mcp.is_some() {
                                    <form method="post" action=(format!("{base}/proposes/{}/review", p.id))>
                                        <input type="hidden" name="csrf" value=(&account.csrf)>
                                        <input type="hidden" name="action" value="dispatch">
                                        <button class="button" type="submit">"Send to agent"</button>
                                    </form>
                                } else {
                                    <button class="button" type="button" disabled="" title="Connect an agent via MCP to send">"Send to agent"</button>
                                }
                            }
                        </footer>

                        <template data-proposal-template=(&p.id)>
                            <div class="proposal-modal-grid">
                                <div class="proposal-stage">
                                    if let Some(art) = &p.artifact {
                                        if matches!(art.kind, proposal_record::ArtifactKind::Html) {
                                            <div class="stage-bar">
                                                <div class="stage-viewports">
                                                    <button class="button" type="button" data-vp="320px">"320px"</button>
                                                    <button class="button" type="button" data-vp="768px">"768px"</button>
                                                    <button class="button" type="button" data-vp="100%">"Full"</button>
                                                </div>
                                                <a class="button" href=(format!("{base}/proposes/{}/preview", p.id)) target="_blank" rel="noreferrer">"Open raw"</a>
                                            </div>
                                            <iframe class="proposal-iframe" sandbox="allow-scripts" src=(format!("{base}/proposes/{}/preview", p.id)) title=(format!("Preview · {}", p.title))></iframe>
                                        } else {
                                            <img class="proposal-hires" src=(format!("{base}/proposes/{}/assets/{}", p.id, art.entry)) alt=(&p.title)>
                                        }
                                    } else if !p.attachments.is_empty() {
                                        <div class="proposal-attachments">
                                            for att in &p.attachments {
                                                if matches!(att.kind, AttachmentKind::Image) {
                                                    <figure>
                                                        <img class="proposal-hires" src=(format!("{base}/proposes/{}/assets/{}", p.id, att.filename)) alt=(&att.filename)>
                                                        <figcaption>(&att.filename)</figcaption>
                                                    </figure>
                                                } else {
                                                    <figure>
                                                        <video controls="" preload="metadata" src=(format!("{base}/proposes/{}/assets/{}", p.id, att.filename))></video>
                                                        <figcaption>(&att.filename)</figcaption>
                                                    </figure>
                                                }
                                            }
                                        </div>
                                    } else {
                                        <div class="empty">
                                            <p>"No rendered preview yet. Send this proposal to a connected MCP agent to extract specs and render a preview."</p>
                                        </div>
                                    }
                                </div>
                                <aside class="proposal-rail">
                                    <div class="propose-badges">
                                        <span class="status">(p.kind.label())</span>
                                        <span class="status">(p.status.label())</span>
                                    </div>
                                    if !p.problem_solved.is_empty() {
                                        <div><h3>"Problem solved"</h3><p>(&p.problem_solved)</p></div>
                                    }
                                    if !p.rationale.is_empty() {
                                        <div><h3>"Why this design"</h3><p class="whitespace-pre-wrap">(&p.rationale)</p></div>
                                    }
                                    if !p.tactics.is_empty() {
                                        <div>
                                            <h3>"Tactics"</h3>
                                            <div class="tactic-chips">
                                                for t in &p.tactics { <span class="tactic-chip">(t)</span> }
                                            </div>
                                        </div>
                                    }
                                    if let Some(ext) = &p.extracted {
                                        <div class="surface panel-padding">
                                            <h3>"Structured extraction"</h3>
                                            if let Some(summary) = &ext.summary { <p>(&summary)</p> }
                                            if let Some(layout) = &ext.layout_structure { <p class="muted">"Layout: " (layout)</p> }
                                            if !ext.palette_hex.is_empty() {
                                                <div class="swatch-row">
                                                    for hex in &ext.palette_hex { <span class="swatch-chip"><code>(hex)</code></span> }
                                                </div>
                                            }
                                            if !ext.target_requirements.is_empty() {
                                                <ul>
                                                    for req in &ext.target_requirements { <li>(req)</li> }
                                                </ul>
                                            }
                                        </div>
                                    }
                                    <details>
                                        <summary>"Prompt & structured brief"</summary>
                                        <pre class="structured-prompt">(proposal_record::structured_prompt(project_id, p, None))</pre>
                                        <button class="button" type="button" data-copy-prompt="">"Copy structured prompt"</button>
                                    </details>
                                    if role == Role::Owner {
                                        <form class="stack-form" method="post" action=(format!("{base}/proposes/{}/review", p.id))>
                                            <input type="hidden" name="csrf" value=(&account.csrf)>
                                            <label for=(format!("comment-{}", p.id))>"Observation / variation note"</label>
                                            <textarea id=(format!("comment-{}", p.id)) name="comment" rows="3" maxlength="2000" placeholder="Add feedback, rejection reason, or requested variation..."></textarea>
                                            <div class="review-actions">
                                                <button class="button primary" type="submit" name="action" value="approve">"Approve"</button>
                                                <button class="button" type="submit" name="action" value="reject">"Reject"</button>
                                                <button class="button" type="submit" name="action" value="variation">"Request variation"</button>
                                                <button class="button" type="submit" name="action" value="comment">"Comment"</button>
                                                if mcp.is_some() {
                                                    <button class="button" type="submit" name="action" value="dispatch">"Send to agent"</button>
                                                }
                                            </div>
                                        </form>
                                    }
                                    if !p.comments.is_empty() {
                                        <ul class="findings-list">
                                            for c in &p.comments {
                                                <li>
                                                    <p class="muted">(&c.author) if let Some(act) = &c.action { " · " (act) } " · " <time data-epoch-ms=(c.timestamp_ms.to_string())></time></p>
                                                    <p>(&c.body)</p>
                                                </li>
                                            }
                                        </ul>
                                    }
                                </aside>
                            </div>
                        </template>
                    </article>
                }
            </div>
        }

        <dialog id="proposal-modal">
            <div class="page-title">
                <h2 data-proposal-title="">"Proposal"</h2>
                <form method="dialog"><button class="button" value="cancel">"Close"</button></form>
            </div>
            <div data-proposal-body=""></div>
        </dialog>
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "the handler keeps validation, attachment handling, and persistence in execution order"
)]
pub async fn create(
    panel: Arc<Panel>,
    account: Account,
    id: String,
    headers: HeaderMap,
    body: Body,
) -> PageResult<Response> {
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    if !safe_name(&id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let bytes = to_bytes(body, MAX_UPLOAD_BYTES)
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    let ctype = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let mut csrf = String::new();
    let mut kind_raw = String::new();
    let mut title = String::new();
    let mut problem_solved = String::new();
    let mut rationale = String::new();
    let mut prompt = String::new();
    let mut surface = String::new();
    let mut refs_raw = String::new();
    let mut tactics = Vec::new();
    let mut files: Vec<(String, Vec<u8>, &'static str, AttachmentKind)> = Vec::new();

    if let Some(boundary) = ctype
        .split(';')
        .find_map(|s| s.trim().strip_prefix("boundary="))
        .map(|b| b.trim_matches('"'))
    {
        for part in parse_multipart(boundary, &bytes)? {
            if let Some(fname) = part.filename {
                if part.data.is_empty() {
                    continue;
                }
                if part.data.len() > MAX_SINGLE_FILE {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                let base_name = Path::new(&fname)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if !safe_asset(base_name) {
                    return Err(StatusCode::UNPROCESSABLE_ENTITY);
                }
                let (mime, akind) =
                    content_type_for(base_name).ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
                if !valid_magic(mime, &part.data) {
                    return Err(StatusCode::UNPROCESSABLE_ENTITY);
                }
                files.push((base_name.to_owned(), part.data, mime, akind));
            } else {
                let val = String::from_utf8(part.data).map_err(|_| StatusCode::BAD_REQUEST)?;
                match part.name.as_str() {
                    "csrf" => csrf = val,
                    "kind" => kind_raw = val,
                    "title" => title = val,
                    "problem_solved" => problem_solved = val,
                    "rationale" => rationale = val,
                    "prompt" => prompt = val,
                    "surface" => surface = val,
                    "references" => refs_raw = val,
                    "tactics" => {
                        let t = val.trim();
                        if !t.is_empty() {
                            tactics.push(t.to_owned());
                        }
                    }
                    _ => {}
                }
            }
        }
    } else {
        for (k, v) in url::form_urlencoded::parse(&bytes) {
            match k.as_ref() {
                "csrf" => csrf = v.into_owned(),
                "kind" => kind_raw = v.into_owned(),
                "title" => title = v.into_owned(),
                "problem_solved" => problem_solved = v.into_owned(),
                "rationale" => rationale = v.into_owned(),
                "prompt" => prompt = v.into_owned(),
                "surface" => surface = v.into_owned(),
                "references" => refs_raw = v.into_owned(),
                "tactics" => {
                    let t = v.trim();
                    if !t.is_empty() {
                        tactics.push(t.to_owned());
                    }
                }
                _ => {}
            }
        }
    }

    auth::check_csrf(&account, &csrf)?;
    let title = title.trim().to_owned();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let ts = now_ms()?;
    let pid = format!("{ts}-{}", std::process::id());
    let root = proposals_root(&panel, &id);
    let assets_dir = root.join(&pid).join("assets");
    let mut attachments = Vec::new();
    for (idx, (fname, data, mime, akind)) in files.into_iter().enumerate() {
        proposal_record::private_dir(&assets_dir).map_err(db_error)?;
        let stored = format!("{idx}-{fname}");
        fs::write(assets_dir.join(&stored), data).map_err(db_error)?;
        attachments.push(Attachment {
            id: format!("att-{idx}"),
            filename: stored,
            content_type: mime.into(),
            kind: akind,
        });
    }
    let references: Vec<String> = refs_raw
        .lines()
        .map(str::trim)
        .filter(|u| {
            !u.is_empty()
                && url::Url::parse(u)
                    .is_ok_and(|parsed| matches!(parsed.scheme(), "http" | "https"))
        })
        .map(str::to_owned)
        .collect();

    let proposal = Proposal {
        id: pid,
        parent_id: None,
        timestamp_ms: ts,
        author: Some(account.handle),
        origin: Origin::User,
        kind: parse_kind(&kind_raw),
        status: Status::PendingExtraction,
        title,
        problem_solved: problem_solved.trim().to_owned(),
        rationale: rationale.trim().to_owned(),
        prompt: prompt.trim().to_owned(),
        surface: Some(surface.trim().to_owned()).filter(|s| !s.is_empty()),
        tactics,
        references,
        attachments,
        artifact: None,
        extracted: None,
        comments: Vec::new(),
        dispatched_at_ms: None,
    };
    proposal_record::write_proposal(&root, &proposal).map_err(db_error)?;
    Ok(redirect(&format!("/projects/{id}/proposes")))
}

#[derive(serde::Deserialize)]
pub struct ReviewForm {
    csrf: String,
    action: String,
    comment: Option<String>,
}

pub fn review(
    panel: &Panel,
    account: Account,
    (id, pid): (String, String),
    form: &ReviewForm,
) -> PageResult<Response> {
    auth::check_csrf(&account, &form.csrf)?;
    if !account.owner {
        return Err(StatusCode::FORBIDDEN);
    }
    if !safe_name(&id) || !proposal_record::safe_id(&pid) {
        return Err(StatusCode::NOT_FOUND);
    }
    let root = proposals_root(panel, &id);
    let path = root.join(format!("{pid}.json"));
    let mut p = proposal_record::read_proposal(&path).map_err(|_| StatusCode::NOT_FOUND)?;
    let ts = now_ms()?;
    let note = form
        .comment
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    if let Some(body) = &note {
        p.comments.push(Comment {
            id: format!("{ts}-{}", std::process::id()),
            author: account.handle.clone(),
            timestamp_ms: ts,
            body: body.clone(),
            action: Some(form.action.clone()),
        });
    }

    match form.action.as_str() {
        "approve" => {
            p.status = Status::Approved;
            proposal_record::write_proposal(&root, &p).map_err(db_error)?;
            promote_to_design(panel, &account, &id, &p)?;
        }
        "reject" => {
            p.status = Status::Rejected;
            proposal_record::write_proposal(&root, &p).map_err(db_error)?;
        }
        "comment" => {
            proposal_record::write_proposal(&root, &p).map_err(db_error)?;
        }
        "variation" => {
            proposal_record::write_proposal(&root, &p).map_err(db_error)?;
            let child_id = format!("{}-{}", ts + 1, std::process::id());
            let mut child_comments = Vec::new();
            if let Some(body) = note {
                child_comments.push(Comment {
                    id: format!("{ts}-var"),
                    author: account.handle.clone(),
                    timestamp_ms: ts,
                    body,
                    action: Some("variation".into()),
                });
            }
            let child = Proposal {
                id: child_id,
                parent_id: Some(p.id.clone()),
                timestamp_ms: ts + 1,
                author: Some(account.handle),
                origin: Origin::User,
                kind: p.kind,
                status: Status::VariationRequested,
                title: format!("{} (variation)", p.title),
                problem_solved: p.problem_solved.clone(),
                rationale: p.rationale.clone(),
                prompt: p.prompt.clone(),
                surface: p.surface.clone(),
                tactics: p.tactics.clone(),
                references: p.references.clone(),
                attachments: Vec::new(),
                artifact: None,
                extracted: None,
                comments: child_comments,
                dispatched_at_ms: None,
            };
            proposal_record::write_proposal(&root, &child).map_err(db_error)?;
            if proposal_record::mcp_session_active(data_dir(panel)).is_some() {
                queue_dispatch(&root, &id, &child, Some(&p))?;
            }
        }
        "dispatch" => {
            if proposal_record::mcp_session_active(data_dir(panel)).is_none() {
                return Err(StatusCode::CONFLICT);
            }
            let parent = p
                .parent_id
                .as_deref()
                .filter(|id| proposal_record::safe_id(id))
                .and_then(|id| {
                    proposal_record::read_proposal(&root.join(format!("{id}.json"))).ok()
                });
            p.dispatched_at_ms = Some(ts);
            proposal_record::write_proposal(&root, &p).map_err(db_error)?;
            queue_dispatch(&root, &id, &p, parent.as_ref())?;
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    }

    Ok(redirect(&format!("/projects/{id}/proposes")))
}

fn queue_dispatch(
    root: &Path,
    project_id: &str,
    p: &Proposal,
    parent: Option<&Proposal>,
) -> PageResult<()> {
    let ts = now_ms()?;
    let did = format!("{ts}-{}", std::process::id());
    let dispatch = Dispatch {
        id: did.clone(),
        proposal_id: p.id.clone(),
        project_id: project_id.to_owned(),
        created_at_ms: ts,
        structured_prompt: proposal_record::structured_prompt(project_id, p, parent),
    };
    proposal_record::write_atomic(
        &root
            .join(proposal_record::DISPATCH_DIR)
            .join(format!("{did}.json")),
        &dispatch,
    )
    .map_err(db_error)
}

fn promote_to_design(
    panel: &Panel,
    account: &Account,
    project_id: &str,
    p: &Proposal,
) -> PageResult<()> {
    let design_root = panel.design.join(project_id);
    fs::create_dir_all(&design_root).map_err(db_error)?;
    if design_record::project(&design_root).is_err() {
        proposal_record::write_atomic(
            &design_root.join("project.json"),
            &serde_json::json!({"id": project_id, "label": project_id}),
        )
        .map_err(db_error)?;
    }
    let kind = match p.kind {
        proposal_record::Kind::Component | proposal_record::Kind::Motion => {
            design_record::Kind::Tactic
        }
        proposal_record::Kind::Palette
        | proposal_record::Kind::DesignSystem
        | proposal_record::Kind::Application
        | proposal_record::Kind::Page
        | proposal_record::Kind::Layout => design_record::Kind::Direction,
        proposal_record::Kind::Other => design_record::Kind::Decision,
    };
    let mut note = String::new();
    if !p.problem_solved.is_empty() {
        writeln!(note, "Problem: {}", p.problem_solved).expect("writing to a String cannot fail");
    }
    if !p.rationale.is_empty() {
        writeln!(note, "Rationale: {}", p.rationale).expect("writing to a String cannot fail");
    }
    if !p.tactics.is_empty() {
        writeln!(note, "Tactics: {}", p.tactics.join(", "))
            .expect("writing to a String cannot fail");
    }
    if let Some(summary) = p.extracted.as_ref().and_then(|e| e.summary.as_deref()) {
        writeln!(note, "Summary: {summary}").expect("writing to a String cannot fail");
    }
    let ts = now_ms()?;
    let entry_id = format!("{ts}-{}", std::process::id());
    let entry = design_record::Entry {
        id: entry_id.clone(),
        timestamp_ms: ts,
        kind,
        status: design_record::Status::Adopted,
        title: format!("{} ({})", p.title, p.kind.label()),
        note: note.trim().to_owned(),
        surface: p.surface.clone(),
        supersedes: None,
        reference: p.references.first().cloned(),
        evidence: None,
        author: Some(account.handle.clone()),
    };
    proposal_record::write_atomic(&design_root.join(format!("{entry_id}.json")), &entry)
        .map_err(db_error)
}

pub async fn preview(
    panel: Arc<Panel>,
    account: Account,
    (id, pid): (String, String),
) -> PageResult<Response> {
    let _ = auth::role(&panel, &account, &id).await?;
    if !safe_name(&id) || !proposal_record::safe_id(&pid) {
        return Err(StatusCode::NOT_FOUND);
    }
    let root = proposals_root(&panel, &id);
    let p = proposal_record::read_proposal(&root.join(format!("{pid}.json")))
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let Some(art) = p.artifact else {
        return Err(StatusCode::NOT_FOUND);
    };
    if !matches!(art.kind, proposal_record::ArtifactKind::Html) || !safe_asset(&art.entry) {
        return Err(StatusCode::NOT_FOUND);
    }
    let file = root.join(&pid).join("assets").join(&art.entry);
    let html = fs::read_to_string(&file).map_err(|_| StatusCode::NOT_FOUND)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src 'self' data: blob:; frame-ancestors 'self'; base-uri 'none'; form-action 'none'",
        )
        .body(Body::from(html))
        .map_err(db_error)
}

pub async fn asset(
    panel: Arc<Panel>,
    account: Account,
    (id, pid, name): (String, String, String),
) -> PageResult<Response> {
    let _ = auth::role(&panel, &account, &id).await?;
    if !safe_name(&id) || !proposal_record::safe_id(&pid) || !safe_asset(&name) {
        return Err(StatusCode::NOT_FOUND);
    }
    let (mime, _) = content_type_for(&name).ok_or(StatusCode::NOT_FOUND)?;
    let file = proposals_root(&panel, &id)
        .join(&pid)
        .join("assets")
        .join(&name);
    let meta = fs::symlink_metadata(&file).map_err(|_| StatusCode::NOT_FOUND)?;
    if !meta.is_file() || meta.len() > MAX_SINGLE_FILE as u64 {
        return Err(StatusCode::NOT_FOUND);
    }
    let bytes = fs::read(&file).map_err(|_| StatusCode::NOT_FOUND)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(bytes))
        .map_err(db_error)
}
