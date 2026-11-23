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
    Child::new(view! { cx =>
            <div class="overview-hero">
                if let Some((href, image, scenario)) = &latest_project_evidence {
                    <section class="studio-proof">
                        <div class="section-head">
                            <h2>"Latest evidence"</h2>
                            <span class="tactic-chip">"1440×900 · verified"</span>
                        </div>
                        <a href=(href)>
                            <div class="studio-proof-image"><img src=(image) alt="" decoding="async"></div>
                            <span>(scenario) <span aria-hidden="true">"↗"</span></span>
                        </a>
                    </section>
                }
                <section class="project-brief">
                    <div class="section-head">
                        <h2>"Design context"</h2>
                        <a class="button" href=(format!("{base}/design"))>"Open design"</a>
                    </div>
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
                                    <span class="tactic-chip">(entry.kind.label())</span>
                                    <strong>(&entry.title)</strong>
                                </li>
                            }
                        </ul>
                    }
                    <p class="muted brief-foot">(current) " active design records"</p>
                </section>
            </div>
            <section class="studio-counts" aria-label="Project pipeline">
                <a class="metric" href=(format!("{base}/captures"))><small>"Captures (" (all_assets.len()) " assets)"</small><strong>(related.len())</strong><span aria-hidden="true">"↗"</span></a>
                <a class=(if open_problems_count > 0 { "metric featured" } else { "metric" }) href=(format!("{base}/problems"))><small>"Problems queue"</small><strong>(open_problems_count)</strong><span aria-hidden="true">"↗"</span></a>
                <a class="metric" href=(format!("{base}/fixes"))><small>"Recorded fixes"</small><strong>(fix_count)</strong><span aria-hidden="true">"↗"</span></a>
            </section>
            <section class="surface captures-ledger">
                <div class="section-head">
                    <h2>"Recent captures"</h2>
                    <a class="button" href=(format!("{base}/captures"))>"Open research hub"</a>
                </div>
                (list)
            </section>
    })
}
