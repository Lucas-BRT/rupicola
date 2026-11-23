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
                    <p class="muted">"MSRV 1.85.0. Runs " <code>"discover"</code> ", " <code>"scan"</code> ", " <code>"capture"</code> ", " <code>"problem"</code> ", " <code>"fix"</code> ", " <code>"propose"</code> ", " <code>"design"</code> ", " <code>"storage"</code> ", " <code>"video"</code> ", and " <code>"audit"</code> "."</p>
                )
                article_card(
                    <h3><code>"rupicola-panel"</code> " (Review App)"</h3>
                    <p class="muted">"Axum/Topcoat + Turso SQLite + Argon2id auth + " <code>"ffmpeg"</code> " runtime for MP4 transcoding, Split Slider, and S3/R2 storage offload."</p>
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
                    code_block(label: "Docker Hub + Dokploy", code: "services:\n  web:\n    image: lucasbrt/rupicola:latest\n    pull_policy: always\n  site:\n    image: lucasbrt/rupicola-site:latest\n    pull_policy: always")
                )
                card(
                    <h2>"Storage Offload"</h2>
                    <p class="muted">"Track disk usage by project, fix, date, and category. Strip raw frame sequences or offload archived fixes to S3, Cloudflare R2, or " <code>"rclone"</code> " remotes."</p>
                    <div class="cta-row">
                        button(href: panel_url, primary: true, external: true, "Open Panel")
                        button(href: "/docs/overview", "CLI Storage Commands")
                    </div>
                )
            )
        )
    })
}
