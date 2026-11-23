//! Markup. A view takes the data a page needs, already loaded, and returns HTML; it calls no
//! backend and changes nothing.

pub mod project;

use crate::Run;
use crate::components::badge::{BadgeVariant, badge};
use topcoat::context::Cx;
use topcoat::view::view;

pub fn run_list<'a>(cx: &'a Cx, runs: &'a [&'a Run], empty: &'a str) -> topcoat::view::Child<'a> {
    topcoat::view::Child::new(view! {
        cx =>
        if runs.is_empty() {
            <div class="empty"><p>(empty)</p></div>
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
                                <time data-epoch-ms=(run.created_at_ms.to_string())></time>
                            </span>
                            <span class="arrow" aria-hidden="true">"↗"</span>
                        </a>
                    </li>
                }
            </ul>
        }
    })
}
