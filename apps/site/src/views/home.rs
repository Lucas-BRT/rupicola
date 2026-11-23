use super::{RELEASES_URL, code_block, escape_html, framework_tags, layout, stage_strip};

pub fn render(panel_url: &str, public_url: &str) -> String {
    let install_cmd = format!("curl -fsSL {public_url}/install.sh | bash");
    let esc_cmd = escape_html(&install_cmd);

    let strip = stage_strip();
    let tags = framework_tags();

    let body = format!(
        r#"<section class="hero">
  <div class="container">
    <p class="eyebrow">Rust-Native Visual QA &amp; AI UI Skill</p>
    <h1>Ship interfaces verified by evidence, not guesswork.</h1>
    <p class="hero-lead">Rupicola combines a modular AI design skill, deterministic Rust inspection tooling, and a self-hosted review panel for web, webview, and native Rust desktop apps.</p>
    <div class="cta-row">
      <a class="btn btn-primary" href="/install">Install CLI &amp; Skill</a>
      <a class="btn" href="/docs">Read Documentation</a>
      <a class="btn" href="{RELEASES_URL}" target="_blank" rel="noopener">Download Releases</a>
    </div>
    <div class="cmd-bar">
      <code>{esc_cmd}</code>
      <button class="btn btn-sm" type="button" data-copy="{esc_cmd}">Copy</button>
    </div>
    <div class="pipeline-strip" aria-label="Visual QA Pipeline">
      {strip}
    </div>
  </div>
</section>

<section class="section">
  <div class="container grid-3">
    <article class="card">
      <h3>Combinatorial Probe</h3>
      <p class="muted">Exercise every route across <code>320×844</code>, <code>768×1024</code>, and <code>1440×900</code> in dark and light themes, extracting DOM trees and transition MP4s.</p>
    </article>
    <article class="card">
      <h3>Agent Progress &amp; Triage</h3>
      <p class="muted">Agents claim problems via CLI or MCP, stream live stage percentages to the panel, and record before/after diffs with zero manual screenshots.</p>
    </article>
    <article class="card">
      <h3>Lean Rust Containers</h3>
      <p class="muted">Workspace binaries split cleanly: CLI (<code>rupicola</code>), media-ready review panel (<code>rupicola-panel</code>), and this static-free site (<code>rupicola-site</code>).</p>
    </article>
  </div>
</section>

<section class="section">
  <div class="container grid-2">
    <div class="card">
      <p class="eyebrow">Framework Coverage</p>
      <h2>13 Rust &amp; Web Stacks</h2>
      <p class="muted"><code>rupicola discover</code> and <code>rupicola scan</code> detect declared crates and inspect literal UI patterns without running Cargo or model APIs.</p>
      <div class="tag-row">
        {tags}
      </div>
    </div>
    <div class="card">
      <p class="eyebrow">Quick Capture</p>
      <h2>Capture a scripted flow in one command</h2>
      {}
    </div>
  </div>
</section>"#,
        code_block(
            "Terminal",
            "rupicola capture scenario.toml --server-url https://rupicola.example --json",
        )
    );

    layout(
        "Visual QA & Design Engineering",
        "Rust-native visual QA pipeline, AI UI skill, and self-hosted review panel.",
        "home",
        panel_url,
        &body,
    )
}
