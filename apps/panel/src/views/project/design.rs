use crate::components::{
    display::moment,
    form::{field, select_field, stack_form, text_area},
};
use crate::{Account, auth::Role, design_system, media, project::AssetOption, records};
use rupicola_core::design_record;
use std::collections::BTreeSet;
use topcoat::{
    Result,
    context::Cx,
    view::{Child, View, attributes, component, view},
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
        (crate::views::design_system::render(cx, account, role, base, design_tab, studio))
        if design_tab == "records" {
            if role == Role::Owner {
                record_editor(base: base, token: &account.csrf, records: records, superseded: superseded, all_assets: all_assets)
            }
            if records.is_empty() {
                <section class="empty">
                    <h2>"No design records yet"</h2>
                    <p>"Ask your agent to record the brief, art direction, tactics and decisions as the design takes shape."</p>
                </section>
            }
            for kind in [design_record::Kind::Brief, design_record::Kind::Direction, design_record::Kind::Tactic, design_record::Kind::Decision] {
                if records.iter().any(|entry| entry.kind == kind && !superseded.contains(entry.id.as_str())) {
                    design_chapter(kind: kind, records: records, superseded: superseded)
                }
            }
            if !superseded.is_empty() {
                history(records: records, superseded: superseded)
            }
        }
    })
}

/// The form that records a design brief, direction, tactic or decision.
#[component]
async fn record_editor(
    base: &str,
    token: &str,
    records: &[design_record::Entry],
    superseded: &BTreeSet<&str>,
    all_assets: &[AssetOption],
) -> Result<impl View> {
    Ok(view! {
        <details class="surface panel-padding design-editor">
            <summary>"Add or revise a design record"</summary>
            stack_form(action: format!("{base}/design/record"), token: token,
                select_field(id: "design-kind", name: "kind", label: "Record type",
                    <option value="brief">"Brief"</option><option value="direction">"Art direction"</option>
                    <option value="tactic">"Tactic"</option><option value="decision">"Decision"</option>
                )
                field(id: "design-title", name: "title", label: "Title",
                    attrs: attributes! { required="" maxlength="200" })
                text_area(id: "design-note", name: "note", label: "Rationale",
                    attrs: attributes! { required="" maxlength="4000" rows="5" })
                select_field(id: "design-status", name: "status", label: "Status",
                    <option value="proposed">"Proposed"</option><option value="adopted">"Adopted"</option><option value="rejected">"Rejected"</option>
                )
                field(id: "design-surface", name: "surface", label: "Surface (optional)",
                    attrs: attributes! { maxlength="200" })
                field(id: "design-reference", name: "reference", label: "Reference URL (optional)",
                    attrs: attributes! { type="url" maxlength="2048" })
                select_field(id: "design-evidence", name: "evidence", label: "Capture evidence (optional)",
                    <option value="">"None"</option>
                    for asset in all_assets.iter().take(80) {
                        <option value=(records::artifact_of(&asset.url))>(asset.kind) " · " (&asset.name) " (" (&asset.scenario) ")"</option>
                    }
                )
                if !records.is_empty() {
                    select_field(id: "design-replaces", name: "supersedes", label: "Replace an active record of the same type (optional)",
                        <option value="">"None"</option>
                        for entry in records.iter().filter(|entry| !superseded.contains(entry.id.as_str())) {
                            <option value=(&entry.id)>(entry.kind.label()) " · " (&entry.title)</option>
                        }
                    )
                }
                <button class="button primary" type="submit">"Save design record"</button>
            )
        </details>
    })
}

/// The active records of one kind.
#[component]
async fn design_chapter(
    kind: design_record::Kind,
    records: &[design_record::Entry],
    superseded: &BTreeSet<&str>,
) -> Result<impl View> {
    Ok(view! {
        <section class="design-chapter">
            <h2>(kind.label())</h2>
            <ul class="findings-list">
                for entry in records.iter().filter(|entry| entry.kind == kind && !superseded.contains(entry.id.as_str())) {
                    record_item(entry: entry)
                }
            </ul>
        </section>
    })
}

/// One design record with its status, author, time and links.
#[component]
async fn record_item(entry: &design_record::Entry) -> Result<impl View> {
    Ok(view! {
        <li>
            <h3>(&entry.title)</h3>
            <p class="muted">(entry.status.label())
                if let Some(surface) = &entry.surface { " · " (surface) }
                if let Some(author) = &entry.author { " · " (author) }
                " · " moment(epoch_ms: entry.timestamp_ms)
            </p>
            <p class="whitespace-pre-wrap">(&entry.note)</p>
            if let Some(url) = entry.reference.as_deref().filter(|url| url.starts_with("https://") || url.starts_with("http://")) {
                <a href=(url) rel="noreferrer">"Design reference"</a>
            }
            if let Some(artifact) = entry.evidence.as_deref() {
                " " <a href=(media::url(artifact))>"Capture evidence"</a>
            }
        </li>
    })
}

/// The records that later ones replaced.
#[component]
async fn history(
    records: &[design_record::Entry],
    superseded: &BTreeSet<&str>,
) -> Result<impl View> {
    Ok(view! {
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
    })
}
