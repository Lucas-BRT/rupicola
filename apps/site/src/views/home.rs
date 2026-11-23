use super::RELEASES_URL;
use crate::components::{
    display::{
        article_card, button, card, code_block, eyebrow, framework_tags, section, stage_strip,
    },
    page::{Nav, shell},
};
use topcoat::{
    Result,
    view::{View, component, view},
};

/// The landing page.
#[component]
pub async fn home_page(panel_url: &str, public_url: &str) -> Result<impl View> {
    let install_cmd = format!("curl -fsSL {public_url}/install.sh | bash");
    Ok(view! {
        shell(
            title: "Visual QA & Design Engineering",
            description: "Rust-native visual QA pipeline, AI UI skill, and self-hosted review panel.",
            active: Nav::Home,
            panel_url: panel_url,
            <section class="hero">
                <div class="container">
                    eyebrow("Rust-Native Visual QA & AI UI Skill")
                    <h1>"Ship interfaces verified by evidence, not guesswork."</h1>
                    <p class="hero-lead">"Rupicola combines a modular AI design skill, deterministic Rust inspection tooling, and a self-hosted review panel for web, webview, and native Rust desktop apps."</p>
                    <div class="cta-row">
                        button(href: "/install", primary: true, "Install CLI & Skill")
                        button(href: "/docs", "Read Documentation")
                        button(href: RELEASES_URL, external: true, "Download Releases")
                    </div>
                    <div class="cmd-bar">
                        <code>(&install_cmd)</code>
                        <button class="btn btn-sm" type="button" data-copy=(&install_cmd)>"Copy"</button>
                    </div>
                    <div class="pipeline-strip" aria-label="Visual QA Pipeline">
                        stage_strip()
                    </div>
                </div>
            </section>
            section(grid: "grid-3",
                article_card(
                    <h3>"Combinatorial Probe"</h3>
                    <p class="muted">"Exercise every route across " <code>"320×844"</code> ", " <code>"768×1024"</code> ", and " <code>"1440×900"</code> " in dark and light themes, extracting DOM trees and transition MP4s."</p>
                )
                article_card(
                    <h3>"Agent Progress & Triage"</h3>
                    <p class="muted">"Agents file and triage problems and record before/after fixes through MCP tools; reviewers approve them in the panel."</p>
                )
                article_card(
                    <h3>"Lean Rust Services"</h3>
                    <p class="muted">"Workspace binaries split cleanly: CLI (" <code>"rupicola"</code> "), API backend (" <code>"rupicola-backend"</code> "), review panel (" <code>"rupicola-panel"</code> "), and this site (" <code>"rupicola-site"</code> ")."</p>
                )
            )
            section(grid: "grid-2",
                card(
                    eyebrow("Framework Coverage")
                    <h2>"13 Rust & Web Stacks"</h2>
                    <p class="muted"><code>"rupicola discover"</code> " and " <code>"rupicola scan"</code> " detect declared crates and inspect literal UI patterns without running Cargo or model APIs."</p>
                    <div class="tag-row">
                        framework_tags()
                    </div>
                )
                card(
                    eyebrow("Quick Capture")
                    <h2>"Capture a scripted flow in one command"</h2>
                    code_block(label: "Terminal", code: "rupicola capture scenario.toml --server-url https://rupicola.example --json")
                )
            )
        )
    })
}
