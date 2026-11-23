//! Small pieces of the panel's own design: counters, section heads, chips and the like. They
//! carry the panel's stylesheet classes, so a page that uses them looks like one that does not.

use super::badge::{BadgeVariant, badge};
use crate::Run;
use topcoat::{
    Result,
    view::{Child, View, component, view},
};

/// A linked counter: `child` is its label, `value` its number.
#[component]
pub async fn metric(
    #[into] href: String,
    value: impl std::fmt::Display + Send,
    #[default] featured: bool,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <a class=(if featured { "metric featured" } else { "metric" }) href=(href)>
            <small>(child)</small>
            <strong>(value.to_string())</strong>
            <span aria-hidden="true">"↗"</span>
        </a>
    })
}

/// A row of [`metric`]s.
#[component]
pub async fn counts(label: &str, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <section class="studio-counts" aria-label=(label)>(child)</section>
    })
}

/// A heading with controls beside it: `child` holds the controls.
#[component]
pub async fn section_head(title: &str, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <div class="section-head">
            <h2>(title)</h2>
            (child)
        </div>
    })
}

/// The message shown where a list has nothing to list.
#[component]
pub async fn empty(message: &str) -> Result<impl View> {
    Ok(view! {
        <div class="empty"><p>(message)</p></div>
    })
}

/// A small outlined label.
#[component]
pub async fn chip(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! { <span class="tactic-chip">(child)</span> })
}

/// The small caps label above a record's title.
#[component]
pub async fn kind_label(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! { <small class="studio-kind">(child)</small> })
}

/// A time shown in the viewer's own zone; the script fills it in from `epoch_ms`.
#[component]
pub async fn moment(epoch_ms: impl std::fmt::Display + Send) -> Result<impl View> {
    Ok(view! { <time data-epoch-ms=(epoch_ms.to_string())></time> })
}

/// A link styled as a button.
#[component]
pub async fn link_button(
    #[into] href: String,
    #[default] primary: bool,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <a class=(if primary { "button primary" } else { "button" }) href=(href)>(child)</a>
    })
}

/// The latest capture's screenshot, linked to its page; `child` is the heading above it.
#[component]
pub async fn proof_card(
    href: &str,
    image: &str,
    scenario: &str,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <section class="studio-proof">
            (child)
            <a href=(href)>
                <div class="studio-proof-image"><img src=(image) alt="" decoding="async"></div>
                <span>(scenario) <span aria-hidden="true">"↗"</span></span>
            </a>
        </section>
    })
}

/// The captures as a list, newest first, or `message` when there are none.
#[component]
pub async fn run_list(runs: &[&Run], message: &str) -> Result<impl View> {
    Ok(view! {
        if runs.is_empty() {
            empty(message: message)
        } else {
            <ul class="run-list">
                for run in runs {
                    <li>
                        <a href=(format!("/run/{}", run.key))>
                            <span class="run-title"><strong>(&run.scenario)</strong><small>(&run.project)</small></span>
                            <span class="run-state">
                                badge(
                                    variant: if run.status == "blocked" { BadgeVariant::Destructive } else { BadgeVariant::Secondary },
                                    (if run.status == "blocked" { "Blocked" } else if run.inspection == "reviewed" { "Reviewed" } else { "Needs review" })
                                )
                                moment(epoch_ms: run.created_at_ms)
                            </span>
                            <span class="arrow" aria-hidden="true">"↗"</span>
                        </a>
                    </li>
                }
            </ul>
        }
    })
}
