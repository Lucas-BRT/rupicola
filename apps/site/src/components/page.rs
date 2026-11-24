//! The page shell every page shares.

use crate::views::{GITHUB_URL, RELEASES_URL};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{Child, View, component, view},
};

/// The entries of the primary navigation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Home,
    Install,
    Docs,
    Architecture,
}

/// One link of the primary navigation; `current` marks the page being shown.
#[component]
async fn nav_link(
    cx: &Cx,
    href: &str,
    current: bool,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <a href=(href) if current { aria-current="page" }>(child)</a>
    })
}

/// A whole page: head, top bar, `child` as the main area, and footer.
#[component]
pub async fn shell(
    cx: &Cx,
    title: &str,
    description: &str,
    active: Nav,
    panel_url: &str,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en" data-theme="dark">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title) " · Rupicola"</title>
                <meta name="description" content=(description)>
                <link rel="stylesheet" href="/site.css">
                <script src="/site.js" defer=""></script>
            </head>
            <body>
                <a class="skip" href="#main">"Skip to content"</a>
                <header class="topbar">
                    <div class="container topbar-inner">
                        <a class="brand" href="/">
                            <span class="mark" aria-hidden="true">"R"</span>
                            <span>"Rupicola"</span>
                        </a>
                        <nav class="nav-links" aria-label="Primary" data-nav-links="">
                            nav_link(href: "/", current: active == Nav::Home, "Overview")
                            nav_link(href: "/install", current: active == Nav::Install, "Install")
                            nav_link(href: "/docs", current: active == Nav::Docs, "Docs")
                            nav_link(href: "/architecture", current: active == Nav::Architecture, "Architecture")
                        </nav>
                        <div class="topbar-actions">
                            <button class="btn btn-sm" type="button" data-theme-toggle="" aria-label="Toggle color theme">"Dark"</button>
                            <a class="btn btn-sm btn-primary topbar-panel-cta" href=(panel_url) target="_blank" rel="noopener">"Open Panel"</a>
                            <button class="btn btn-sm nav-toggle" type="button" data-nav-toggle="" aria-expanded="false" aria-label="Toggle navigation">"Menu"</button>
                        </div>
                    </div>
                </header>
                <main id="main" tabindex="-1">(child)</main>
                <footer class="footer">
                    <div class="container footer-inner">
                        <span>"Rupicola — Rust-native visual QA & design engineering."</span>
                        <div class="footer-links">
                            <a href="/install">"Install"</a>
                            <a href="/docs">"Docs"</a>
                            <a href="/architecture">"Self-Host"</a>
                            <a href=(RELEASES_URL) target="_blank" rel="noopener">"Releases"</a>
                            <a href=(GITHUB_URL) target="_blank" rel="noopener">"GitHub"</a>
                        </div>
                    </div>
                </footer>
            </body>
        </html>
    })
}
