//! Markup. A view takes the data a page needs, already loaded, and returns HTML; it calls no
//! backend and changes nothing.

use topcoat::view::view;

macro_rules! shell {
    ($cx:ident, $page:expr, $account:ident, $panel:ident, $active:expr, { $($content:tt)* }) => {
        view! {
            $cx =>
            <!DOCTYPE html>
            <html lang="en">
                <head>
                    <meta charset="utf-8">
                    <meta name="viewport" content="width=device-width, initial-scale=1">
                    <title>($page) " · Rupicola"</title>
                    <link rel="stylesheet" href="/panel.css">
                    <script src="/panel.js" defer=""></script>
                </head>
                <body>
                    <a class="skip" href="#main">"Skip to content"</a>
                    <div class="workspace">
                        <header class="topbar">
                            <div class="topbar-inner">
                                <a class="brand" href="/dashboard">
                                    <span class="mark" aria-hidden="true">"R"</span>
                                    <strong>"Rupicola"</strong>
                                    <span class="workspace-tag">(if $panel.secure { "CLOUD" } else { "LOCAL" })</span>
                                </a>
                                <nav aria-label="Workspace">
                                    <a href="/dashboard" if $active == "dashboard" { aria-current="page" }>"Dashboard"</a>
                                    <a href="/projects" if $active == "projects" { aria-current="page" }>"Projects"</a>
                                    <a href="/settings" if $active == "settings" { aria-current="page" }>"Settings"</a>
                                </nav>
                                <div class="topbar-account">
                                    <a href="/settings">(&$account.handle)</a>
                                    <form method="post" action="/logout">
                                        <input type="hidden" name="csrf" value=(&$account.csrf)>
                                        <button class="link-button" type="submit">"Sign out"</button>
                                    </form>
                                </div>
                            </div>
                        </header>
                        <main id="main" tabindex="-1">$($content)*</main>
                    </div>
                </body>
            </html>
        }
    };
}

pub mod auth;
pub mod credential;
pub mod dashboard;
pub mod design_system;
pub mod project;
pub mod projects;
pub mod run;
pub mod settings;

use crate::Run;
use crate::components::badge::{BadgeVariant, badge};
use topcoat::context::Cx;

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
