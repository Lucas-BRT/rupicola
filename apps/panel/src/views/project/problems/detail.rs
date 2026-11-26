use super::super::badge;
use super::{problem_analysis, problem_meta, problem_status_badge, severity_badge};
use crate::Account;
use crate::auth::Role;
use crate::components::{
    display::{chip, kind_label, link_button, moment},
    form::{csrf, field, text_area},
};
use crate::records::Problem;
use topcoat::{
    Result,
    context::{Cx, context},
    view::{View, attributes, component, view},
};

/// Full detail page for a single problem (`/projects/{project}/problems/{problem}`).
#[component]
pub async fn problem_detail(
    cx: &Cx,
    role: Role,
    base: &str,
    item: Problem,
    selections: Vec<rupicola_api::Selection>,
) -> Result<impl View> {
    let account = context::<Account>(cx);
    let token = &account.csrf;
    let meta_chip = problem_meta(&item);
    Ok(view! {
        <div class="propose-toolbar">
            <a class="button button-ghost" href=(format!("{base}/problems")) data-nav-partial="">"← All problems"</a>
            moment(epoch_ms: item.created_at_ms)
        </div>
        <article class="surface problem-card" id=(format!("problem-{}", item.id)) data-problem-card=(&item.id)>
            <header class="propose-head">
                <div>
                    <div class="propose-badges">
                        badge(variant: severity_badge(item.severity), (item.severity.label()))
                        badge(variant: problem_status_badge(item.status), (item.status.label()))
                        if !meta_chip.is_empty() {
                            chip((meta_chip))
                        }
                        chip((item.origin.label()))
                    </div>
                    <h2>(&item.title)</h2>
                </div>
            </header>
            if !item.evidence_assets.is_empty() {
                <div class="problem-evidence-strip">
                    for ev in &item.evidence_assets {
                        <a class="problem-thumb" href=(&ev.url) data-viewer=(&item.title)>
                            <img src=(&ev.url) alt=(&ev.label) loading="lazy">
                            <span>(&ev.label) " ↗"</span>
                        </a>
                    }
                </div>
            }
            if !item.violated_rules.is_empty() {
                <div class="problem-rules">
                    for violation in &item.violated_rules {
                        <span class=(if violation.scope == "project" { "rule-chip project-rule" } else { "rule-chip global-rule" })>
                            <strong>(if violation.scope == "project" { "Project Norm" } else { "Global Norm" })</strong>
                            " · " (&violation.code) " — " (&violation.title)
                        </span>
                    }
                </div>
            }
            problem_analysis(diagnosis: &item.diagnosis, solution: &item.proposed_solution)
            if !selections.is_empty() {
                problem_selections(
                    base: base,
                    problem_id: &item.id,
                    selections: &selections,
                    can_uncite: role != Role::Reader,
                    token: token,
                )
            }
            if !item.comments.is_empty() {
                <ul class="problem-comments">
                    for c in &item.comments {
                        <li>
                            <strong>(&c.author)</strong> ": " (&c.text)
                            " " moment(epoch_ms: c.created_at_ms)
                        </li>
                    }
                </ul>
            }
            <footer class="propose-foot">
                <div class="review-actions">
                    if let Some(fid) = &item.linked_fix_id {
                        link_button(href: format!("{base}/fixes#fix-{fid}"), primary: true, "Compare Before/After Fix ↗")
                    }
                    if role != Role::Reader {
                        <form method="post" action=(format!("{base}/problems/{}/triage", item.id)) class="review-actions">
                            csrf(token: token)
                            <button class="button primary" type="submit" name="status" value="in-progress">"Queue for fix"</button>
                            <button class="button" type="submit" name="status" value="intentional" title="Never flag this route/rule again">"Mark Intentional"</button>
                            <button class="button" type="submit" name="status" value="false-positive">"False positive"</button>
                            <button class="button" type="submit" name="status" value="needs-reprocess">"Reprocess"</button>
                        </form>
                    }
                </div>
                if role != Role::Reader {
                    <div class="review-actions">
                        <details class="triage-note-drawer">
                            <summary>"Refine solution / add triage note"</summary>
                            <form method="post" action=(format!("{base}/problems/{}/triage", item.id)) class="stack-form">
                                csrf(token: token)
                                <input type="hidden" name="status" value="needs-reprocess">
                                text_area(id: format!("sol-{}", item.id), name: "solution", label: "Updated solution proposal", value: &item.proposed_solution,
                                    attrs: attributes! { rows="2" maxlength="2000" })
                                field(id: format!("com-{}", item.id), name: "comment", label: "Triage note / guidance for agent",
                                    attrs: attributes! { maxlength="1000" placeholder="Clarify intent or constraints…" })
                                <button class="button" type="submit">"Save & flag for reprocess"</button>
                            </form>
                        </details>
                    </div>
                }
            </footer>
        </article>
        image_viewer()
    })
}

/// Cited selections on a problem detail page, with optional uncite action.
#[component]
async fn problem_selections(
    cx: &Cx,
    base: &str,
    problem_id: &str,
    selections: &[rupicola_api::Selection],
    can_uncite: bool,
    token: &str,
) -> Result<impl View> {
    let triage_url = format!("{base}/problems/{problem_id}/triage");
    Ok(view! {
        <section class="problem-box">
            kind_label("CITED SELECTIONS")
            <ul class="audit">
                for sel in selections {
                    <li>
                        <span>
                            <strong>(&sel.label)</strong>
                            " · "
                            (format!("{} anchor{}", sel.items.len(), if sel.items.len() == 1 { "" } else { "s" }))
                            for anchor in sel.items.iter().take(3) {
                                " · "
                                <a href=(format!("/run/{}", anchor.capture)) class="mono">(&anchor.capture)</a>
                            }
                        </span>
                        if can_uncite {
                            <form method="post" action=(&triage_url) class="inline-action">
                                csrf(token: token)
                                <input type="hidden" name="action" value="uncite-selection">
                                <input type="hidden" name="selection_id" value=(&sel.id)>
                                <button class="button button-ghost" type="submit">"Uncite"</button>
                            </form>
                        }
                    </li>
                }
            </ul>
        </section>
    })
}

/// The dialog that shows an evidence image at full size.
#[component]
pub(super) async fn image_viewer(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <dialog id="image-viewer" aria-labelledby="image-title">
            <div class="section-head">
                <h2 id="image-title">"Evidence"</h2>
                <form method="dialog"><button class="button" type="submit" autofocus="">"Close"</button></form>
            </div>
            <img alt="">
            <div class="section-head">
                <div class="filters">
                    <button type="button" class="button" data-image-prev="">"Previous"</button>
                    <button type="button" class="button" data-image-next="">"Next"</button>
                </div>
                <a data-image-original="" href=".">"Open original"</a>
            </div>
        </dialog>
    })
}
