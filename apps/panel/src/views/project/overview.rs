use crate::components::display::{
    chip, counts, link_button, metric, proof_card, run_list, section_head,
};
use crate::{Run, project::Proof, records};
use rupicola_api::{DesignEntry, DesignKind};
use std::collections::BTreeSet;
use topcoat::{
    Result,
    context::Cx,
    view::{View, component, view},
};

/// What the overview shows besides the project's name.
pub struct Overview<'a> {
    pub base: &'a str,
    pub latest_project_evidence: Option<Proof>,
    pub records: Vec<DesignEntry>,
    pub captures: usize,
    pub assets: usize,
    pub open_problems_count: usize,
    pub fix_count: usize,
    pub recent: Vec<&'a Run>,
}

/// The overview tab's content.
#[component]
pub async fn overview_tab(cx: &Cx, data: Overview<'_>) -> Result<impl View> {
    let Overview {
        base,
        latest_project_evidence,
        records,
        captures,
        assets,
        open_problems_count,
        fix_count,
        recent,
    } = data;
    Ok(view! {
        let superseded: BTreeSet<&str> = records::superseded_design(&records);
        let current = records
            .iter()
            .filter(|entry| !superseded.contains(entry.id.as_str()))
            .count();
        counts(label: "Project pipeline",
            metric(href: format!("{base}/captures"), value: captures, "Captures (" (assets.to_string()) " assets)")
            metric(href: format!("{base}/problems"), value: open_problems_count, featured: open_problems_count > 0, "Problems queue")
            metric(href: format!("{base}/fixes"), value: fix_count, "Recorded fixes")
        )
        <div class="overview-hero">
            if let Some(proof) = &latest_project_evidence {
                <section class="canvas-stage">
                    proof_card(href: &proof.href, image: &proof.image, scenario: &proof.scenario,
                        section_head(title: "Latest evidence", chip("Visual Stage"))
                    )
                </section>
            }
            <section class="project-brief">
                section_head(title: "Design context",
                    link_button(href: format!("{base}/design"), "Open design")
                )
                if let Some(entry) = records.iter().find(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.entry.kind, DesignKind::Brief | DesignKind::Direction)) {
                    <div class="brief-featured">
                        <p class="studio-kind">(entry.entry.kind.label()) " · " (entry.entry.status.label())</p>
                        <h3>(&entry.entry.title)</h3>
                        <p class="project-brief-note">(&entry.entry.note)</p>
                    </div>
                }
                if records.iter().any(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.entry.kind, DesignKind::Tactic | DesignKind::Decision)) {
                    <ul class="brief-ledger">
                        for entry in records.iter().filter(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.entry.kind, DesignKind::Tactic | DesignKind::Decision)).take(3) {
                            <li>
                                chip((entry.entry.kind.label()))
                                <strong>(&entry.entry.title)</strong>
                            </li>
                        }
                    </ul>
                }
                <p class="muted brief-foot">(current.to_string()) " active design records"</p>
            </section>
        </div>
        <section class="surface captures-ledger">
            section_head(title: "Recent captures",
                link_button(href: format!("{base}/captures"), "Open research hub")
            )
            run_list(runs: &recent, hide_project: true, message: "No matching captures. Send a capture with this project's ID.")
        </section>
    })
}
