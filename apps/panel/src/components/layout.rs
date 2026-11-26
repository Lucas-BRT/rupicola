//! The page chrome: the workspace shell and the headings every page opens with.

use crate::{Account, Panel};
use topcoat::context::Cx;
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

impl Nav {
    /// The entry a page at `path` belongs to.
    #[must_use]
    pub fn of(path: &str) -> Self {
        if path.starts_with("/settings") {
            Self::Settings
        } else if path.starts_with("/projects") || path.starts_with("/run/") {
            Self::Projects
        } else {
            Self::Dashboard
        }
    }
}

/// Bespoke Rupicola crest mark (Guianan cock-of-the-rock silhouette).
#[component]
pub async fn brand_mark(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <span class="mark" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path d="M5 18.5C5 11.2 9.2 5.5 15.8 4.2c1.1-.2 2.1.7 1.9 1.8-.3 1.7-1 3.1-2.1 4.2l3.4 1.6c.6.3.5 1.1-.1 1.3l-3.7 1.1c-.4 2.4-2.4 4.3-5.1 4.3H5Z" fill="currentColor"/>
                <circle cx="13.8" cy="9.2" r="1.15" fill="var(--bird-crest)"/>
            </svg>
        </span>
    })
}

/// The whole document: head with the page's metadata, unified left rail shell with brand,
/// workspace navigation, and bottom-left account/utility dock, plus the main area holding `child`.
#[component]
pub async fn document(
    cx: &Cx,
    account: &Account,
    panel: &Panel,
    active: Nav,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    let _ = panel;
    let initial = account
        .handle
        .chars()
        .next()
        .map_or('?', |ch| ch.to_ascii_uppercase());
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                topcoat::metadata::tags()
                <link rel="stylesheet" href="/panel.css">
                <script src="/panel.js" defer=""></script>
            </head>
            <body>
                <a class="skip" href="#main">"Skip to content"</a>
                <div class="workspace">
                    <aside class="workspace-rail" aria-label="Workspace">
                        <nav class="rail-dock rail-top" aria-label="Workspace navigation">
                            <a class="rail-brand" href="/dashboard" aria-label="Rupicola dashboard" title="Dashboard" if active == Nav::Dashboard { aria-current="page" }>
                                brand_mark()
                                <span class="sr-only">"Dashboard"</span>
                                <span class="rail-tooltip" aria-hidden="true">"Dashboard"</span>
                            </a>
                            <a href="/projects" aria-label="Projects" title="Projects" if active == Nav::Projects { aria-current="page" }>
                                <svg class="rail-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                    <path d="M3 7a2 2 0 0 1 2-2h4.6a2 2 0 0 1 1.4.58l1.42 1.42a2 2 0 0 0 1.41.58H19a2 2 0 0 1 2 2v8.42a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z"/>
                                </svg>
                                <span class="sr-only">"Projects"</span>
                                <span class="rail-tooltip" aria-hidden="true">"Projects"</span>
                            </a>
                        </nav>
                        <div class="rail-dock rail-account" aria-label="Account and preferences">
                            <button class="rail-control" type="button" data-theme-toggle="" aria-label="Switch to light theme" title="Switch to light theme">
                                <svg class="rail-icon icon-sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                    <circle cx="12" cy="12" r="4"/>
                                    <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/>
                                </svg>
                                <svg class="rail-icon icon-moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                    <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z"/>
                                </svg>
                                <span class="sr-only">"Switch theme"</span>
                                <span class="rail-tooltip" aria-hidden="true" data-theme-tooltip="">"Light theme"</span>
                            </button>
                            <a href="/settings" aria-label="Account settings" title="Account settings" if active == Nav::Settings { aria-current="page" }>
                                <svg class="rail-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                                    <path d="M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"/>
                                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.08a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.08a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"/>
                                </svg>
                                <span class="sr-only">"Account settings"</span>
                                <span class="rail-tooltip" aria-hidden="true">"Account settings"</span>
                            </a>
                            <details class="account-menu">
                                <summary class="rail-avatar" aria-label=(format!("Account {}", account.handle)) title=(&account.handle)>
                                    <span aria-hidden="true">(initial)</span>
                                    <span class="rail-tooltip" aria-hidden="true">(&account.handle)</span>
                                </summary>
                                <div class="account-popover">
                                    <div class="account-popover-head">
                                        <strong>(&account.handle)</strong>
                                        <small class="muted">(if account.owner { "Administrator" } else { "Member" })</small>
                                    </div>
                                    <a class="account-popover-link" href="/settings">"Account settings"</a>
                                    <form method="post" action="/logout">
                                        <input type="hidden" name="csrf" value=(&account.csrf)>
                                        <button class="account-popover-signout" type="submit">"Sign out"</button>
                                    </form>
                                </div>
                            </details>
                        </div>
                    </aside>
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
    cx: &Cx,
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
pub async fn breadcrumbs(cx: &Cx, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <nav class="breadcrumbs" aria-label="Breadcrumb">(child)</nav>
    })
}

/// A row of tab links; `class` adds to `tabs`.
#[component]
pub async fn tabs(
    cx: &Cx,
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
    cx: &Cx,
    #[into] href: String,
    current: bool,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <a href=(href) if current { aria-current="page" }>(child)</a>
    })
}
