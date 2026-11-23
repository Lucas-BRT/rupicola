use crate::components::{
    display::{article_card, button, card, code_block, eyebrow, section},
    page::{Nav, shell},
};
use topcoat::{
    Result,
    view::{View, component, view},
};

/// The architecture and self-hosting page.
#[component]
pub async fn architecture_page(panel_url: &str) -> Result<impl View> {
    Ok(view! {
        shell(
            title: "Architecture",
            description: "Rupicola workspace architecture, container security, and Dokploy deployment.",
            active: Nav::Architecture,
            panel_url: panel_url,
            section(
                eyebrow("Architecture & Self-Hosting")
                <h1>"Split Workspace, Hardened Containers"</h1>
                <p class="hero-lead">"Each runtime concern lives in its own Cargo crate so CI compiles once and Dokploy pulls lean, non-root images."</p>
            )
            section(grid: "grid-2",
                article_card(
                    <h3><code>"rupicola"</code> " (CLI & Core)"</h3>
                    <p class="muted">"MSRV 1.85.0. Runs " <code>"discover"</code> ", " <code>"scan"</code> ", " <code>"capture"</code> ", " <code>"publish"</code> ", " <code>"video"</code> ", " <code>"audit"</code> ", and the " <code>"mcp-bridge"</code> " to the backend."</p>
                )
                article_card(
                    <h3><code>"rupicola-backend"</code> " (API)"</h3>
                    <p class="muted">"Axum + SQLite: accounts, projects, captures, problems, fixes, design records, the media store and the MCP endpoint at " <code>"/mcp"</code> "."</p>
                )
                article_card(
                    <h3><code>"rupicola-panel"</code> " (Review App)"</h3>
                    <p class="muted">"A Topcoat web client of the backend: sessions, review, Split Slider and Pixel Diff. It keeps only design-system versions of its own."</p>
                )
                article_card(
                    <h3><code>"rupicola-site"</code> " (Landing & Docs)"</h3>
                    <p class="muted">"Embedded markdown + CSS/JS in a single binary with zero " <code>"ffmpeg"</code> ", zero database, and read-only container filesystem."</p>
                )
                article_card(
                    <h3>"Container Hardening"</h3>
                    <p class="muted">"Non-root UID " <code>"10001"</code> ", " <code>"read_only: true"</code> ", " <code>"cap_drop: ALL"</code> ", " <code>"no-new-privileges"</code> ", memory/PID limits, and self-contained " <code>"--healthcheck"</code> "."</p>
                )
            )
            section(grid: "grid-2",
                card(
                    <h2>"CI/CD Image Pipeline"</h2>
                    <p class="muted">"GitHub Actions builds " <code>"linux/amd64"</code> " images with BuildKit + GHA cache, pushes to Docker Hub, and triggers Dokploy via webhook."</p>
                    code_block(label: "Docker Hub + Dokploy", code: "services:\n  backend:\n    image: lucasbrt/rupicola-backend:latest\n    pull_policy: always\n  web:\n    image: lucasbrt/rupicola:latest\n    pull_policy: always\n  site:\n    image: lucasbrt/rupicola-site:latest\n    pull_policy: always")
                )
                card(
                    <h2>"Agents & the API"</h2>
                    <p class="muted">"Agents reach the backend through " <code>"rupicola mcp-bridge"</code> ", authenticated with a token the backend issued; the panel and the CLI use the same API."</p>
                    <div class="cta-row">
                        button(href: panel_url, primary: true, external: true, "Open Panel")
                        button(href: "/docs/overview", "CLI Reference")
                    </div>
                )
            )
        )
    })
}
