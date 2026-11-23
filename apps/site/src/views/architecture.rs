use super::{code_block, escape_html, layout};

pub fn render(panel_url: &str) -> String {
    let compose_snippet = code_block(
        "Docker Hub + Dokploy",
        "services:\n  web:\n    image: lucasbrt/rupicola:latest\n    pull_policy: always\n  site:\n    image: lucasbrt/rupicola-site:latest\n    pull_policy: always",
    );

    let body = format!(
        r#"<section class="section">
  <div class="container">
    <p class="eyebrow">Architecture &amp; Self-Hosting</p>
    <h1>Split Workspace, Hardened Containers</h1>
    <p class="hero-lead">Each runtime concern lives in its own Cargo crate so CI compiles once and Dokploy pulls lean, non-root images.</p>
  </div>
</section>

<section class="section">
  <div class="container grid-2">
    <article class="card">
      <h3><code>rupicola</code> (CLI &amp; Core)</h3>
      <p class="muted">MSRV 1.85.0. Runs <code>discover</code>, <code>scan</code>, <code>capture</code>, <code>problem</code>, <code>fix</code>, <code>propose</code>, <code>design</code>, <code>storage</code>, <code>video</code>, and <code>audit</code>.</p>
    </article>
    <article class="card">
      <h3><code>rupicola-panel</code> (Review App)</h3>
      <p class="muted">Axum/Topcoat + Turso SQLite + Argon2id auth + <code>ffmpeg</code> runtime for MP4 transcoding, Split Slider, and S3/R2 storage offload.</p>
    </article>
    <article class="card">
      <h3><code>rupicola-site</code> (Landing &amp; Docs)</h3>
      <p class="muted">Embedded markdown + CSS/JS in a single binary with zero <code>ffmpeg</code>, zero database, and read-only container filesystem.</p>
    </article>
    <article class="card">
      <h3>Container Hardening</h3>
      <p class="muted">Non-root UID <code>10001</code>, <code>read_only: true</code>, <code>cap_drop: ALL</code>, <code>no-new-privileges</code>, memory/PID limits, and self-contained <code>--healthcheck</code>.</p>
    </article>
  </div>
</section>

<section class="section">
  <div class="container grid-2">
    <div class="card">
      <h2>CI/CD Image Pipeline</h2>
      <p class="muted">GitHub Actions builds <code>linux/amd64</code> images with BuildKit + GHA cache, pushes to Docker Hub, and triggers Dokploy via webhook.</p>
      {compose_snippet}
    </div>
    <div class="card">
      <h2>Storage Offload</h2>
      <p class="muted">Track disk usage by project, fix, date, and category. Strip raw frame sequences or offload archived fixes to S3, Cloudflare R2, or <code>rclone</code> remotes.</p>
      <div class="cta-row">
        <a class="btn btn-primary" href="{esc_panel}" target="_blank" rel="noopener">Open Panel</a>
        <a class="btn" href="/docs/overview">CLI Storage Commands</a>
      </div>
    </div>
  </div>
</section>"#,
        esc_panel = escape_html(panel_url)
    );

    layout(
        "Architecture",
        "Rupicola workspace architecture, container security, and Dokploy deployment.",
        "arch",
        panel_url,
        &body,
    )
}
