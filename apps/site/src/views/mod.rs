//! Markup shared by every page: the page shell, navigation and small components.

mod architecture;
mod docs;
mod home;
mod install;

pub use architecture::render as architecture;
pub use docs::render as docs;
pub use home::render as home;
pub use install::render as install;

use std::fmt::Write;

pub const GITHUB_URL: &str = "https://github.com/Lucas-BRT/rupicola";
pub const RELEASES_URL: &str = "https://github.com/Lucas-BRT/rupicola/releases";

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

pub fn nav_link(href: &str, label: &str, active: &str, key: &str) -> String {
    let current = if active == key {
        r#" aria-current="page""#
    } else {
        ""
    };
    format!(r#"<a href="{href}"{current}>{label}</a>"#)
}

pub fn layout(title: &str, description: &str, active: &str, panel_url: &str, body: &str) -> String {
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

pub fn code_block(label: &str, code: &str) -> String {
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
pub fn stage_strip() -> String {
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
pub fn framework_tags() -> String {
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
