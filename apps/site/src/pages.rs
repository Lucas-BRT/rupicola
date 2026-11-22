use std::fmt::Write;

use crate::docs::{DOCS, DocPage, render_markdown};

const GITHUB_URL: &str = "https://github.com/Lucas-BRT/rupicola";
const RELEASES_URL: &str = "https://github.com/Lucas-BRT/rupicola/releases";

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn nav_link(href: &str, label: &str, active: &str, key: &str) -> String {
    let current = if active == key {
        r#" aria-current="page""#
    } else {
        ""
    };
    format!(r#"<a href="{href}"{current}>{label}</a>"#)
}

fn layout(title: &str, description: &str, active: &str, panel_url: &str, body: &str) -> String {
    let esc_title = escape_html(title);
    let esc_desc = escape_html(description);
    let esc_panel = escape_html(panel_url);

    let nav = [
        nav_link("/", "Overview", active, "home"),
        nav_link("/install", "Install", active, "install"),
        nav_link("/docs", "Docs", active, "docs"),
        nav_link("/architecture", "Architecture", active, "arch"),
    ]
    .join("\n          ");

    format!(
        r##"<!DOCTYPE html>
<html lang="en" data-theme="dark">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{esc_title} · Rupicola</title>
  <meta name="description" content="{esc_desc}">
  <link rel="stylesheet" href="/site.css">
  <script src="/site.js" defer></script>
</head>
<body>
  <a class="skip" href="#main">Skip to content</a>
  <header class="topbar">
    <div class="container topbar-inner">
      <a class="brand" href="/">
        <span class="mark" aria-hidden="true">R</span>
        <span>Rupicola</span>
      </a>
      <nav class="nav-links" aria-label="Primary" data-nav-links>
        {nav}
      </nav>
      <div class="topbar-actions">
        <button class="btn btn-sm" type="button" data-theme-toggle aria-label="Toggle color theme">Dark</button>
        <a class="btn btn-sm btn-primary" href="{esc_panel}" target="_blank" rel="noopener">Panel</a>
        <button class="btn btn-sm nav-toggle" type="button" data-nav-toggle aria-expanded="false" aria-label="Toggle navigation">Menu</button>
      </div>
    </div>
  </header>
  <main id="main" tabindex="-1">
    {body}
  </main>
  <footer class="footer">
    <div class="container footer-inner">
      <span>Rupicola — Rust-native visual QA &amp; design engineering.</span>
      <div class="footer-links">
        <a href="/install">Install</a>
        <a href="/docs">Docs</a>
        <a href="/architecture">Self-Host</a>
        <a href="{RELEASES_URL}" target="_blank" rel="noopener">Releases</a>
        <a href="{GITHUB_URL}" target="_blank" rel="noopener">GitHub</a>
      </div>
    </div>
  </footer>
</body>
</html>"##
    )
}

fn code_block(label: &str, code: &str) -> String {
    let esc_label = escape_html(label);
    let esc_code = escape_html(code);
    format!(
        r#"<div class="code-block">
  <div class="code-head">
    <span>{esc_label}</span>
    <button class="btn btn-sm" type="button" data-copy="{esc_code}">Copy</button>
  </div>
  <pre><code>{esc_code}</code></pre>
</div>"#
    )
}

/// The six-stage strip under the hero: capture to storage.
fn stage_strip() -> String {
    let stages = [
        (
            "01",
            "Captures",
            "Headless Playwright matrix across viewports, themes, and routes.",
        ),
        (
            "02",
            "Problems",
            "Structured queue with live agent claims and task progress.",
        ),
        (
            "03",
            "Fixes",
            "Before/after PNG &amp; MP4 evidence with Split Slider and Pixel Diff.",
        ),
        (
            "04",
            "Proposes",
            "Side-by-side HTML/image visual directions for design review.",
        ),
        (
            "05",
            "Design",
            "Persistent art direction, palette, tactics, and decision log.",
        ),
        (
            "06",
            "Storage",
            "By-Fix/Date/Run disk breakdown with S3, R2, and rclone offload.",
        ),
    ];

    let mut strip = String::new();
    for (num, name, desc) in stages {
        let _ = write!(
            strip,
            r#"<div class="stage-cell">
  <span class="stage-num">{num}</span>
  <strong>{name}</strong>
  <p>{desc}</p>
</div>"#
        );
    }
    strip
}

/// One tag per UI framework the skill covers.
fn framework_tags() -> String {
    let frameworks = [
        "Topcoat",
        "Leptos",
        "Dioxus",
        "Yew",
        "Iced",
        "GPUI",
        "egui",
        "Slint",
        "GTK4",
        "libadwaita",
        "Relm4",
        "libcosmic",
        "Tauri",
    ];
    let tags: String = frameworks
        .iter()
        .map(|f| format!(r#"<span class="tag">{f}</span>"#))
        .collect::<Vec<_>>()
        .join("\n          ");
    tags
}

pub fn render_home(panel_url: &str) -> String {
    let install_cmd = "curl -fsSL https://rupicola.lucasbrt.dev/install.sh | bash";
    let esc_cmd = escape_html(install_cmd);

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
      <p class="eyebrow">Quick Probe</p>
      <h2>Verify any local URL in one command</h2>
      {}
    </div>
  </div>
</section>"#,
        code_block(
            "Terminal",
            "rupicola probe --url http://127.0.0.1:4732 --mode exhaustive --pages all --viewports 320x844,768x1024,1440x900 --themes dark,light --json",
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

pub fn render_install(panel_url: &str) -> String {
    let quick_block = code_block(
        "Quick Install (Linux / macOS)",
        "curl -fsSL https://rupicola.lucasbrt.dev/install.sh | bash\nrupicola --version",
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

pub fn render_docs(doc: &DocPage, panel_url: &str) -> String {
    let mut sidebar_groups = String::new();
    let mut last_group = "";
    for item in DOCS {
        if item.group != last_group {
            if !last_group.is_empty() {
                sidebar_groups.push_str("</ul></div>");
            }
            last_group = item.group;
            let _ = write!(
                sidebar_groups,
                r#"<div><div class="docs-group-title">{}</div><ul class="docs-nav-list">"#,
                escape_html(item.group)
            );
        }
        let active_attr = if item.slug == doc.slug {
            r#" aria-current="page""#
        } else {
            ""
        };
        let _ = write!(
            sidebar_groups,
            r#"<li data-doc-item="{} {}"><a href="/docs/{}"{}>{}</a></li>"#,
            escape_html(item.title),
            escape_html(item.group),
            escape_html(item.slug),
            active_attr,
            escape_html(item.title)
        );
    }
    if !last_group.is_empty() {
        sidebar_groups.push_str("</ul></div>");
    }

    let rendered = render_markdown(doc);
    let body = format!(
        r#"<div class="container docs-layout">
  <aside class="docs-sidebar" aria-label="Documentation navigation">
    <input id="docs-filter" class="docs-search" type="search" placeholder="Filter docs..." aria-label="Filter documentation">
    {sidebar_groups}
  </aside>
  <article class="prose">
    <div class="breadcrumb">
      <span>Docs</span>
      <span aria-hidden="true">/</span>
      <span>{}</span>
      <span aria-hidden="true">/</span>
      <code>{}</code>
    </div>
    {rendered}
  </article>
</div>"#,
        escape_html(doc.group),
        escape_html(doc.source_path)
    );

    layout(
        doc.title,
        "Rupicola documentation, workflows, UX principles, and framework guides.",
        "docs",
        panel_url,
        &body,
    )
}

pub fn render_architecture(panel_url: &str) -> String {
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
      <p class="muted">MSRV 1.85.0. Runs <code>discover</code>, <code>scan</code>, <code>probe</code>, <code>problem</code>, <code>fix</code>, <code>propose</code>, <code>design</code>, <code>storage</code>, <code>video</code>, and <code>audit</code>.</p>
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
