use super::{HeadingStyle, Outcome, Phase, Studio, Version};
use crate::{Account, auth::Role};
use topcoat::{
    context::Cx,
    view::{Child, view},
};

fn version_url(base: &str, tab: &str, id: &str) -> String {
    format!("{base}/design?view={tab}&version={id}")
}

pub fn render<'a>(
    cx: &'a Cx,
    account: &'a Account,
    role: Role,
    base: &'a str,
    tab: &'a str,
    studio: &'a Studio,
) -> Child<'a> {
    let action = format!("{base}/design/version");
    Child::new(view! { cx =>
        <nav class="tabs design-tabs" aria-label="Design">
            <a href=(format!("{base}/design")) if tab == "records" { aria-current="page" }>"Overview"</a>
            for (key, title) in [("system", "System"), ("screens", "Screens"), ("proposals", "Proposals")] {
                <a href=(studio.selected.as_ref().map_or_else(|| format!("{base}/design?view={key}"), |version| version_url(base, key, &version.id)))
                    if tab == key { aria-current="page" }>(title)</a>
            }
        </nav>
        if tab == "records" {
            if let Some(active) = &studio.active {
                <p><a href=(version_url(base, "system", active))>"Open adopted design system"</a></p>
            }
        } else {
            <section class="surface panel-padding design-studio">
                <div class="design-studio-head">
                    <h2>"System versions"</h2>
                    if role == Role::Owner {
                        <form method="post" action=(&action)>
                            <input type="hidden" name="csrf" value=(&account.csrf)>
                            <button class="button" type="submit" name="action" value="create">"New draft"</button>
                        </form>
                    }
                </div>
                if studio.items.is_empty() {
                    <p class="muted">"No design system yet."</p>
                } else {
                    <ul class="version-list">
                        for (id, name, phase) in &studio.items {
                            <li>
                                <a href=(version_url(base, tab, id)) if studio.selected.as_ref().is_some_and(|version| &version.id == id) { aria-current="page" }>(name)</a>
                                <small>(if studio.active.as_ref() == Some(id) { "Adopted" } else { phase.label() })</small>
                            </li>
                        }
                    </ul>
                }
            </section>
            if let Some(version) = &studio.selected {
                if tab == "system" {
                    (document(cx, account, base, role, version))
                } else if tab == "screens" {
                    (screens(cx, account, base, role, version, studio.active.as_deref()))
                } else {
                    (proposal(cx, account, base, role, version, studio.active.as_deref()))
                }
            }
        }
    })
}

fn form_fields<'a>(cx: &'a Cx, account: &'a Account, version: &'a Version) -> Child<'a> {
    Child::new(view! { cx =>
        <input type="hidden" name="csrf" value=(&account.csrf)>
        <input type="hidden" name="version" value=(&version.id)>
        <input type="hidden" name="generation" value=(version.generation.to_string())>
    })
}

fn document<'a>(
    cx: &'a Cx,
    account: &'a Account,
    base: &'a str,
    role: Role,
    version: &'a Version,
) -> Child<'a> {
    let doc = &version.document;
    Child::new(view! { cx =>
        <article class="surface panel-padding design-studio">
            <div class="design-studio-head">
                <h2>(&doc.name)</h2>
                <a class="button" href=(version_url(base, "screens", &version.id))>"Open component library"</a>
            </div>
            <div class="design-doc-grid">
                <section><h3>"Identity and direction"</h3><p>(&doc.direction)</p></section>
                <section><h3>"Usage and content"</h3><p>(&doc.usage)</p></section>
                <section>
                    <h3>"Foundations"</h3>
                    <dl class="design-tokens">
                        for (label, color) in [("Canvas", &doc.canvas), ("Surface", &doc.surface), ("Text", &doc.ink), ("Primary action", &doc.accent), ("Primary action text", &doc.action_ink), ("Success", &doc.success), ("Error", &doc.error)] {
                            <div><dt>(label)</dt><dd><span class="design-swatch" style=(format!("background:{color}")) aria-hidden="true"></span><code>(color)</code></dd></div>
                        }
                        <div><dt>"Base spacing"</dt><dd>(format!("{} px", doc.spacing))</dd></div>
                    </dl>
                    <p>(doc.heading.label()) " headings · system sans for controls and body text"</p>
                    <p>(format!("Corner radius: {} px", doc.radius))</p>
                </section>
                <section>
                    <h3>"Components and patterns"</h3>
                    <dl class="design-patterns">
                        <dt>"Button"</dt><dd>(&doc.guidance.button)</dd>
                        <dt>"Review form"</dt><dd>(&doc.guidance.form)</dd>
                        <dt>"Evidence table"</dt><dd>(&doc.guidance.table)</dd>
                        <dt>"Comparison"</dt><dd>(&doc.guidance.comparison)</dd>
                    </dl>
                </section>
            </div>
            if version.phase == Phase::Draft && role == Role::Owner {
                (editor(cx, account, base, version))
            }
        </article>
    })
}

fn editor<'a>(cx: &'a Cx, account: &'a Account, base: &'a str, version: &'a Version) -> Child<'a> {
    let doc = &version.document;
    Child::new(view! { cx =>
        <details class="design-editor">
            <summary>"Edit document and screen"</summary>
            <form class="stack-form" method="post" action=(format!("{base}/design/edit"))>
                (form_fields(cx, account, version))
                <label for="system-name">"Name"</label><input id="system-name" name="name" value=(&doc.name) required="" maxlength="120">
                <label for="system-direction">"Identity and direction"</label><textarea id="system-direction" name="direction" rows="3" required="" maxlength="2500">(&doc.direction)</textarea>
                <label for="system-usage">"Usage and content"</label><textarea id="system-usage" name="usage" rows="3" required="" maxlength="2500">(&doc.usage)</textarea>
                <div class="design-fields">
                    for (name, label, color) in [("canvas", "Canvas", &doc.canvas), ("surface", "Surface", &doc.surface), ("ink", "Text", &doc.ink), ("accent", "Primary action", &doc.accent), ("action_ink", "Primary action text", &doc.action_ink), ("success", "Success", &doc.success), ("error", "Error", &doc.error)] {
                        <label>(label)<input type="color" name=(name) value=(color) required=""></label>
                    }
                    <label>"Base spacing (px)"<input type="number" name="spacing" min="8" max="32" value=(doc.spacing.to_string()) required=""></label>
                    <label>"Corner radius (px)"<input type="number" name="radius" min="0" max="16" value=(doc.radius.to_string()) required=""></label>
                    <label>"Heading typography"<select name="heading">
                        <option value="editorial" if doc.heading == HeadingStyle::Editorial { selected="" }>"Editorial serif"</option>
                        <option value="product" if doc.heading == HeadingStyle::Product { selected="" }>"Product sans"</option>
                    </select></label>
                </div>
                <h3>"Template content"</h3>
                <div class="design-fields">
                    <label>"Workspace name"<input name="brand" value=(&doc.brand) required="" maxlength="120"></label>
                    <label>"Home title"<input name="landing_title" value=(&doc.landing_title) required="" maxlength="120"></label>
                    <label>"Work queue title"<input name="queue_title" value=(&doc.queue_title) required="" maxlength="120"></label>
                    <label>"First queue item"<input name="queue_item" value=(&doc.queue_item) required="" maxlength="120"></label>
                    <label>"Settings title"<input name="settings_title" value=(&doc.settings_title) required="" maxlength="120"></label>
                </div>
                <label for="system-screen">"Evidence review title"</label><input id="system-screen" name="screen_title" value=(&doc.screen_title) required="" maxlength="120">
                <label for="system-scenario">"Example context"</label><input id="system-scenario" name="scenario" value=(&doc.scenario) required="" maxlength="200">
                <div class="design-fields">
                    <label>"Before label"<input name="before_label" value=(&doc.before_label) required="" maxlength="48"></label>
                    <label>"After label"<input name="after_label" value=(&doc.after_label) required="" maxlength="48"></label>
                </div>
                <details>
                    <summary>"Component guidance"</summary>
                    <label>"Buttons"<textarea name="button_notes" rows="3" required="" maxlength="1400">(&doc.guidance.button)</textarea></label>
                    <label>"Review form"<textarea name="form_notes" rows="3" required="" maxlength="1400">(&doc.guidance.form)</textarea></label>
                    <label>"Evidence table"<textarea name="table_notes" rows="3" required="" maxlength="1400">(&doc.guidance.table)</textarea></label>
                    <label>"Comparison"<textarea name="comparison_notes" rows="3" required="" maxlength="1400">(&doc.guidance.comparison)</textarea></label>
                </details>
                <button type="submit" class="button primary">"Save draft"</button>
            </form>
        </details>
    })
}

fn screens<'a>(
    cx: &'a Cx,
    account: &'a Account,
    base: &'a str,
    role: Role,
    version: &'a Version,
    active: Option<&'a str>,
) -> Child<'a> {
    let preview = format!("{base}/design/preview/{}", version.id);
    let ready = version.ready(active);
    Child::new(view! { cx =>
        <section class="surface panel-padding design-studio">
            <div class="design-studio-head">
                <h2>"Components and example pages"</h2>
                <a class="button" href=(&preview) target="_blank" rel="noreferrer">"Open full preview"</a>
            </div>
            <nav class="tabs" aria-label="Open an example in a new tab">
                for (key, label) in [("components", "Components"), ("home", "Home"), ("queue", "Work queue"), ("settings", "Settings"), ("evidence", "Evidence review")] {
                    <a href=(format!("{preview}#{key}")) target="_blank" rel="noreferrer">(label)</a>
                }
            </nav>
            <iframe class="design-preview" src=(&preview) sandbox="allow-scripts" title="Design system examples"></iframe>
        </section>
        <section class="surface panel-padding design-studio">
            <div class="design-studio-head">
                <h2>"Validation"</h2>
                if version.phase == Phase::Draft && role == Role::Owner {
                    <form method="post" action=(format!("{base}/design/version"))>
                        (form_fields(cx, account, version))
                        <button class="button primary" type="submit" name="action" value="validate">"Validate examples"</button>
                    </form>
                }
            </div>
            if let Some(report) = &version.report {
                <p role="status">(match report.outcome {
                    Outcome::Passed if version.phase == Phase::Draft && !ready => "Outdated. Validate this version again.",
                    Outcome::Passed => "Required checks passed.",
                    Outcome::Failed => "Validation failed.",
                    Outcome::Blocked => "Validation blocked.",
                })</p>
                if let Some(error) = &report.error { <p>(&error)</p> }
                if let Some(browser) = &report.browser { <p class="muted">"Chromium " (browser)</p> }
                <ul>
                    for check in &report.checks { <li>(check)</li> }
                </ul>
                if report.baseline.is_some() {
                    <p>"Baseline captures use the adopted version with this draft’s example data."</p>
                }
                <div class="actions">
                    for shot in &report.shots {
                        <a href=(format!("{base}/design/proof/{}/{}", version.id, shot.name)) target="_blank" rel="noreferrer">(&shot.name)</a>
                    }
                </div>
            } else {
                <p class="muted">"Validate the saved draft to send it for review."</p>
            }
        </section>
    })
}

fn proposal<'a>(
    cx: &'a Cx,
    account: &'a Account,
    base: &'a str,
    role: Role,
    version: &'a Version,
    active: Option<&'a str>,
) -> Child<'a> {
    let adopted = active == Some(version.id.as_str());
    Child::new(view! { cx =>
        <section class="surface panel-padding design-studio">
            <div class="design-studio-head">
                <h2>(&version.document.name)</h2>
                <span>(if adopted { "Adopted" } else { version.phase.label() })</span>
            </div>
            <p class="muted">"Created by " (&version.author)
                if let Some(parent) = &version.parent {
                    " · " <a href=(version_url(base, "proposals", parent))>"Parent version"</a>
                }
            </p>
            if role == Role::Owner {
                (review_form(cx, account, base, version, adopted, active))
            }
            if !version.notes.is_empty() {
                <h3>"Review history"</h3>
                <ol class="design-notes">
                    for note in &version.notes {
                        <li><strong>(&note.actor)</strong> " · " (&note.action) " · " (&note.target)
                            <time data-epoch-ms=(note.time.saturating_mul(1000).to_string())></time>
                            if !note.text.is_empty() { <p>(&note.text)</p> }
                        </li>
                    }
                </ol>
            }
        </section>
    })
}

fn review_form<'a>(
    cx: &'a Cx,
    account: &'a Account,
    base: &'a str,
    version: &'a Version,
    adopted: bool,
    active: Option<&'a str>,
) -> Child<'a> {
    Child::new(view! { cx =>
        <form class="stack-form" method="post" action=(format!("{base}/design/version"))>
            (form_fields(cx, account, version))
            if version.phase == Phase::Draft {
                <div class="actions">
                    if version.ready(active) {
                        <button type="submit" class="button primary" name="action" value="submit">"Send for review"</button>
                    } else {
                        <a href=(version_url(base, "screens", &version.id))>"Validate before sending for review"</a>
                    }
                </div>
            } else {
                <label for="review-target">"Comment target"</label>
                <select id="review-target" name="target">
                    for (key, title) in [("document", "Whole document"), ("identity", "Identity"), ("tokens", "Tokens"), ("components", "Components"), ("comparison", "Comparison screen"), ("media-unavailable", "Unavailable media"), ("form", "Review form")] {
                        <option value=(key)>(title)</option>
                    }
                </select>
                <label for="review-note">"Note"</label>
                <textarea id="review-note" name="note" rows="3" maxlength="4000"></textarea>
                <div class="actions">
                    <button type="submit" class="button" name="action" value="comment">"Add comment"</button>
                    if version.phase == Phase::Review {
                        <button type="submit" class="button primary" name="action" value="approve">"Approve"</button>
                        <button type="submit" class="button" name="action" value="changes">"Request changes"</button>
                    }
                    if version.phase == Phase::Approved && !adopted {
                        <button type="submit" class="button primary" name="action" value="adopt">"Adopt system"</button>
                    }
                    <button type="submit" class="button" name="action" value="variation">"Create variation"</button>
                </div>
            }
        </form>
    })
}
