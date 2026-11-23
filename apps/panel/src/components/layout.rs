//! The page chrome: the workspace shell and the headings every page opens with.

use crate::{Account, Panel};
use topcoat::{
    Result,
    view::{Child, View, component, view},
};

/// The entries of the workspace navigation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Dashboard,
    Projects,
    Settings,
}

/// The whole page: document head, top bar with navigation and account, and the main area
/// holding `child`.
#[component]
pub async fn workspace(
    title: &str,
    account: &Account,
    panel: &Panel,
    active: Nav,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title) " · Rupicola"</title>
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
                                <span class="workspace-tag">(if panel.secure { "CLOUD" } else { "LOCAL" })</span>
                            </a>
                            <nav aria-label="Workspace">
                                <a href="/dashboard" if active == Nav::Dashboard { aria-current="page" }>"Dashboard"</a>
                                <a href="/projects" if active == Nav::Projects { aria-current="page" }>"Projects"</a>
                                <a href="/settings" if active == Nav::Settings { aria-current="page" }>"Settings"</a>
                            </nav>
                            <div class="topbar-account">
                                <a href="/settings">(&account.handle)</a>
                                <form method="post" action="/logout">
                                    <input type="hidden" name="csrf" value=(&account.csrf)>
                                    <button class="link-button" type="submit">"Sign out"</button>
                                </form>
                            </div>
                        </div>
                    </header>
                    <main id="main" tabindex="-1">(child)</main>
                </div>
            </body>
        </html>
    })
}

/// The heading a page opens with; `class` adds to `page-title`, `child` holds what sits
/// beside the title.
#[component]
pub async fn page_title(
    title: &str,
    #[default] class: &str,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <header class=(format!("page-title {class}").trim_end().to_owned())>
            <h1>(title)</h1>
            (child)
        </header>
    })
}

/// The trail above a project page.
#[component]
pub async fn breadcrumbs(#[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <nav class="breadcrumbs" aria-label="Breadcrumb">(child)</nav>
    })
}

/// A row of tab links; `class` adds to `tabs`.
#[component]
pub async fn tabs(
    label: &str,
    #[default] class: &str,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <nav class=(format!("tabs {class}").trim_end().to_owned()) aria-label=(label)>(child)</nav>
    })
}

/// One link of a [`tabs`] row; `current` marks the tab being shown.
#[component]
pub async fn tab_link(
    #[into] href: String,
    current: bool,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <a href=(href) if current { aria-current="page" }>(child)</a>
    })
}
