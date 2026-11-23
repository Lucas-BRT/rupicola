use super::{RELEASES_URL, code_block, layout};

pub fn render(panel_url: &str, public_url: &str) -> String {
    let quick_block = code_block(
        "Quick Install (Linux / macOS)",
        &format!("curl -fsSL {public_url}/install.sh | bash\nrupicola --version"),
    );
    let source_block = code_block(
        "Cargo (Git)",
        "cargo install --git https://github.com/Lucas-BRT/rupicola.git --locked rupicola\nrupicola --version",
    );
    let release_block = code_block(
        "Linux / macOS Release Archive",
        "gh release download <tag> --repo Lucas-BRT/ferris-atelier --pattern 'rupicola-<tag>-<target>.tar.gz'\ntar -xzf rupicola-<tag>-<target>.tar.gz\n./rupicola/rupicola --version",
    );
    let skill_unix = code_block(
        "Link Skill (Linux / macOS)",
        "mkdir -p ~/.agents/skills ~/.claude/skills\nln -s \"$PWD\" ~/.agents/skills/rupicola\nln -s \"$PWD\" ~/.claude/skills/rupicola",
    );
    let skill_win = code_block(
        "Link Skill (Windows PowerShell)",
        "New-Item -ItemType Directory -Force \"$HOME\\.agents\\skills\", \"$HOME\\.claude\\skills\" | Out-Null\nNew-Item -ItemType Junction -Path \"$HOME\\.agents\\skills\\rupicola\" -Target (Get-Location).Path\nNew-Item -ItemType Junction -Path \"$HOME\\.claude\\skills\\rupicola\" -Target (Get-Location).Path",
    );
    let agents_md = code_block(
        "AGENTS.md",
        "Use Rupicola for visual or interaction changes. Capture and inspect the affected UI states, recapture after authorized fixes, and report any blocked checks. API-only work is outside this workflow.",
    );

    let body = format!(
        r#"<section class="section">
  <div class="container">
    <p class="eyebrow">Installation</p>
    <h1>Install CLI, Releases &amp; AI Skill</h1>
    <p class="hero-lead">Minimum Rust version for the CLI is <code>1.85.0</code>. Core inspection runs offline with zero API keys; Playwright is only used for headless web probing.</p>
    <div class="cta-row">
      <a class="btn btn-primary" href="/install.sh" download="install.sh">Download install.sh</a>
      <a class="btn" href="{RELEASES_URL}" target="_blank" rel="noopener">GitHub Releases</a>
      <a class="btn" href="/docs/overview">Full CLI Reference</a>
    </div>
  </div>
</section>

<section class="section">
  <div class="container">
    <div class="card">
      <h2>Quick Install Script</h2>
      <p class="muted">Downloads the matching release archive, verifies <code>SHA256SUMS</code>, links <code>rupicola</code> into <code>~/.local/bin</code>, and registers the skill when Codex/Claude directories exist (falls back to <code>cargo install</code> if no release is published yet).</p>
      {quick_block}
    </div>
  </div>
</section>

<section class="section">
  <div class="container grid-2">
    <div class="card">
      <h2>1. Install from Source</h2>
      <p class="muted">Requires Git and Rust 1.85+. Installs <code>rupicola</code> into <code>~/.cargo/bin</code>.</p>
      {source_block}
    </div>
    <div class="card">
      <h2>2. Prebuilt Release Binaries</h2>
      <p class="muted">Built by CI for <code>x86_64-unknown-linux-gnu</code>, <code>x86_64-pc-windows-msvc</code>, <code>aarch64-apple-darwin</code>, and <code>x86_64-apple-darwin</code> with <code>SHA256SUMS</code>.</p>
      {release_block}
    </div>
  </div>
</section>

<section class="section">
  <div class="container grid-2">
    <div class="card">
      <h2>3. Register the AI Skill</h2>
      <p class="muted">Keep a checkout or extracted release folder and symlink it as <code>rupicola</code> for Codex and Claude Code.</p>
      {skill_unix}
      {skill_win}
    </div>
    <div class="card">
      <h2>4. Project Opt-In</h2>
      <p class="muted">Add this instruction to your repository's <code>AGENTS.md</code> so agents invoke Rupicola on UI work.</p>
      {agents_md}
    </div>
  </div>
</section>"#
    );

    layout(
        "Install",
        "Install Rupicola CLI from Cargo or GitHub Releases and register the AI skill.",
        "install",
        panel_url,
        &body,
    )
}
