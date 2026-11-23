use crate::{Account, auth::Role, design_system, media, project::AssetOption, records};
use rupicola_core::design_record;
use std::collections::BTreeSet;
use topcoat::{
    context::Cx,
    view::{Child, view},
};

/// The design tab: the records, and the design-system studio below them.
#[derive(Clone, Copy)]
pub struct Design<'a> {
    pub base: &'a str,
    pub role: Role,
    pub tab: &'a str,
    pub records: &'a [design_record::Entry],
    pub superseded: &'a BTreeSet<&'a str>,
    pub all_assets: &'a [AssetOption],
    pub studio: &'a design_system::Studio,
}

pub fn render<'a>(cx: &'a Cx, account: &'a Account, data: Design<'a>) -> Child<'a> {
    let Design {
        base,
        role,
        tab,
        records,
        superseded,
        all_assets,
        studio,
    } = data;
    let design_tab = tab;
    Child::new(view! { cx =>
        (design_system::render(cx, account, role, base, design_tab, studio))
        if design_tab == "records" {
            if role == Role::Owner {
                <details class="surface panel-padding design-editor">
                    <summary>"Add or revise a design record"</summary>
                    <form class="stack-form" method="post" action=(format!("{base}/design/record"))>
                        <input type="hidden" name="csrf" value=(&account.csrf)>
                        <label for="design-kind">"Record type"</label>
                        <select id="design-kind" name="kind">
                            <option value="brief">"Brief"</option><option value="direction">"Art direction"</option>
                            <option value="tactic">"Tactic"</option><option value="decision">"Decision"</option>
                        </select>
                        <label for="design-title">"Title"</label><input id="design-title" name="title" required="" maxlength="200">
                        <label for="design-note">"Rationale"</label><textarea id="design-note" name="note" required="" maxlength="4000" rows="5"></textarea>
                        <label for="design-status">"Status"</label>
                        <select id="design-status" name="status"><option value="proposed">"Proposed"</option><option value="adopted">"Adopted"</option><option value="rejected">"Rejected"</option></select>
                        <label for="design-surface">"Surface (optional)"</label><input id="design-surface" name="surface" maxlength="200">
                        <label for="design-reference">"Reference URL (optional)"</label><input id="design-reference" name="reference" type="url" maxlength="2048">
                        <label for="design-evidence">"Capture evidence (optional)"</label>
                        <select id="design-evidence" name="evidence"><option value="">"None"</option>
                            for asset in all_assets.iter().take(80) {
                                <option value=(records::artifact_of(&asset.url))>(asset.kind) " · " (&asset.name) " (" (&asset.scenario) ")"</option>
                            }
                        </select>
                        if !records.is_empty() {
                            <label for="design-replaces">"Replace an active record of the same type (optional)"</label>
                            <select id="design-replaces" name="supersedes"><option value="">"None"</option>
                                for entry in records.iter().filter(|entry| !superseded.contains(entry.id.as_str())) {
                                    <option value=(&entry.id)>(entry.kind.label()) " · " (&entry.title)</option>
                                }
                            </select>
                        }
                        <button class="button primary" type="submit">"Save design record"</button>
                    </form>
                </details>
            }
            if records.is_empty() {
                <section class="empty">
                    <h2>"No design records yet"</h2>
                    <p>"Ask your agent to record the brief, art direction, tactics and decisions as the design takes shape."</p>
                </section>
            }
            for kind in [design_record::Kind::Brief, design_record::Kind::Direction, design_record::Kind::Tactic, design_record::Kind::Decision] {
                if records.iter().any(|entry| entry.kind == kind && !superseded.contains(entry.id.as_str())) {
                    <section class="design-chapter">
                        <h2>(kind.label())</h2>
                        <ul class="findings-list">
                            for entry in records.iter().filter(|entry| entry.kind == kind && !superseded.contains(entry.id.as_str())) {
                                <li>
                                    <h3>(&entry.title)</h3>
                                    <p class="muted">(entry.status.label())
                                        if let Some(surface) = &entry.surface { " · " (surface) }
                                        if let Some(author) = &entry.author { " · " (author) }
                                        " · " <time data-epoch-ms=(entry.timestamp_ms.to_string())></time>
                                    </p>
                                    <p class="whitespace-pre-wrap">(&entry.note)</p>
                                    if let Some(url) = entry.reference.as_deref().filter(|url| url.starts_with("https://") || url.starts_with("http://")) {
                                        <a href=(url) rel="noreferrer">"Design reference"</a>
                                    }
                                    if let Some(artifact) = entry.evidence.as_deref() {
                                        " " <a href=(media::url(artifact))>"Capture evidence"</a>
                                    }
                                </li>
                            }
                        </ul>
                    </section>
                }
            }
            if !superseded.is_empty() {
                <section class="surface panel-padding">
                    <details>
                        <summary>"Decision and design history"</summary>
                        <ul class="findings-list">
                            for entry in records.iter().filter(|entry| superseded.contains(entry.id.as_str())) {
                                <li>
                                    <h3>(&entry.title)</h3>
                                    <p class="muted">(entry.kind.label()) " · " (entry.status.label()) " · Superseded"</p>
                                    <p class="whitespace-pre-wrap">(&entry.note)</p>
                                </li>
                            }
                        </ul>
                    </details>
                </section>
            }
        }
    })
}
