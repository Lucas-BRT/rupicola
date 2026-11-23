use crate::components::display::{chip, counts, link_button, metric, proof_card, section_head};
use crate::{Run, project::AssetOption};
use rupicola_core::design_record;
use std::collections::BTreeSet;
use topcoat::{
    context::Cx,
    view::{Child, view},
};

/// What the overview shows besides the project's name.
pub struct Overview<'a> {
    pub base: &'a str,
    pub latest_project_evidence: Option<(String, String, String)>,
    pub records: &'a [design_record::Entry],
    pub superseded: &'a BTreeSet<&'a str>,
    pub current: usize,
    pub related: &'a [&'a Run],
    pub all_assets: &'a [AssetOption],
    pub open_problems_count: usize,
    pub fix_count: usize,
    pub list: Child<'a>,
}

pub fn render<'a>(cx: &'a Cx, data: Overview<'a>) -> Child<'a> {
    let Overview {
        base,
        latest_project_evidence,
        records,
        superseded,
        current,
        related,
        all_assets,
        open_problems_count,
        fix_count,
        list,
    } = data;
    Child::new(view! {
        <div class="overview-hero">
            if let Some((href, image, scenario)) = &latest_project_evidence {
                proof_card(href: href, image: image, scenario: scenario,
                    section_head(title: "Latest evidence", chip("1440×900 · verified"))
                )
            }
            <section class="project-brief">
                section_head(title: "Design context",
                    link_button(href: format!("{base}/design"), "Open design")
                )
                if let Some(entry) = records.iter().find(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.kind, design_record::Kind::Brief | design_record::Kind::Direction)) {
                    <div class="brief-featured">
                        <p class="studio-kind">(entry.kind.label()) " · " (entry.status.label())</p>
                        <h3>(&entry.title)</h3>
                        <p class="project-brief-note">(&entry.note)</p>
                    </div>
                }
                if records.iter().any(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.kind, design_record::Kind::Tactic | design_record::Kind::Decision)) {
                    <ul class="brief-ledger">
                        for entry in records.iter().filter(|entry| !superseded.contains(entry.id.as_str()) && matches!(entry.kind, design_record::Kind::Tactic | design_record::Kind::Decision)).take(3) {
                            <li>
                                chip((entry.kind.label()))
                                <strong>(&entry.title)</strong>
                            </li>
                        }
                    </ul>
                }
                <p class="muted brief-foot">(current) " active design records"</p>
            </section>
        </div>
        counts(label: "Project pipeline",
            metric(href: format!("{base}/captures"), value: related.len(), "Captures (" (all_assets.len()) " assets)")
            metric(href: format!("{base}/problems"), value: open_problems_count, featured: open_problems_count > 0, "Problems queue")
            metric(href: format!("{base}/fixes"), value: fix_count, "Recorded fixes")
        )
        <section class="surface captures-ledger">
            section_head(title: "Recent captures",
                link_button(href: format!("{base}/captures"), "Open research hub")
            )
            (list)
        </section>
    })
}
