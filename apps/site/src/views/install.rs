use super::RELEASES_URL;
use crate::components::{
    display::{button, card, code_block, eyebrow, section},
    page::{Nav, shell},
};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// The install page.
#[component]
pub async fn install_page(cx: &Cx, panel_url: &str, public_url: &str) -> Result<impl View> {
    let quick = format!("curl -fsSL {public_url}/install.sh | bash\nrupicola --version");
    Ok(view! {
        shell(
            title: "Install",
            description: "Install Rupicola CLI from Cargo or GitHub Releases and register the AI skill.",
            active: Nav::Install,
            panel_url: panel_url,
            section(
                eyebrow("Installation")
                <h1>"Install CLI, Releases & AI Skill"</h1>
                <p class="hero-lead">"Minimum Rust version for the CLI is " <code>"1.99"</code> ". Core inspection runs offline with zero API keys; capture storage and MCP live on the backend."</p>
                <div class="cta-row">
                    button(href: "/install.sh", primary: true, download: Some("install.sh"), "Download install.sh")
                    button(href: RELEASES_URL, external: true, "GitHub Releases")
                    button(href: "/docs/overview", "Full CLI Reference")
                </div>
            )
            section(
                card(
                    <h2>"Quick Install Script"</h2>
                    <p class="muted">"Downloads the matching release archive, verifies " <code>"SHA256SUMS"</code> ", links " <code>"rupicola"</code> " into " <code>"~/.local/bin"</code> ", and registers the skill when Codex/Claude directories exist (falls back to " <code>"cargo install"</code> " if no release is published yet)."</p>
                    code_block(label: "Quick Install (Linux / macOS)", code: &quick)
                )
            )
            section(grid: "grid-2",
                card(
                    <h2>"1. Install from Source"</h2>
                    <p class="muted">"Requires Git and Rust 1.99+. Installs " <code>"rupicola"</code> " into " <code>"~/.cargo/bin"</code> "."</p>
                    code_block(label: "Cargo (Git)", code: "cargo install --git https://github.com/Lucas-BRT/rupicola.git --locked rupicola\nrupicola --version")
                )
                card(
                    <h2>"2. Prebuilt Release Binaries"</h2>
                    <p class="muted">"Built by CI for " <code>"x86_64-unknown-linux-gnu"</code> ", " <code>"x86_64-pc-windows-msvc"</code> ", " <code>"aarch64-apple-darwin"</code> ", and " <code>"x86_64-apple-darwin"</code> " with " <code>"SHA256SUMS"</code> "."</p>
                    code_block(label: "Linux / macOS Release Archive", code: "gh release download <tag> --repo Lucas-BRT/rupicola --pattern 'rupicola-<tag>-<target>.tar.gz'\ntar -xzf rupicola-<tag>-<target>.tar.gz\n./rupicola/rupicola --version")
                )
            )
            section(grid: "grid-2",
                card(
                    <h2>"3. Register the AI Skill"</h2>
                    <p class="muted">"Keep a checkout or extracted release folder and symlink it as " <code>"rupicola"</code> " for Codex and Claude Code."</p>
                    code_block(label: "Link Skill (Linux / macOS)", code: "mkdir -p ~/.agents/skills ~/.claude/skills\nln -s \"$PWD\" ~/.agents/skills/rupicola\nln -s \"$PWD\" ~/.claude/skills/rupicola")
                    code_block(label: "Link Skill (Windows PowerShell)", code: "New-Item -ItemType Directory -Force \"$HOME\\.agents\\skills\", \"$HOME\\.claude\\skills\" | Out-Null\nNew-Item -ItemType Junction -Path \"$HOME\\.agents\\skills\\rupicola\" -Target (Get-Location).Path\nNew-Item -ItemType Junction -Path \"$HOME\\.claude\\skills\\rupicola\" -Target (Get-Location).Path")
                )
                card(
                    <h2>"4. Project Opt-In"</h2>
                    <p class="muted">"Add this instruction to your repository's " <code>"AGENTS.md"</code> " so agents invoke Rupicola on UI work."</p>
                    code_block(label: "AGENTS.md", code: "Use Rupicola for visual or interaction changes. Capture and inspect the affected UI states, recapture after authorized fixes, and report any blocked checks. API-only work is outside this workflow.")
                )
            )
        )
    })
}
