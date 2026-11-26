use super::RELEASES_URL;
use crate::components::{
    display::{button, code_block, framework_tags, section},
    page::{Nav, shell},
};
use topcoat::context::Cx;
use topcoat::{
    Result,
    view::{View, component, view},
};

/// The landing page.
#[component]
pub async fn home_page(cx: &Cx, panel_url: &str, public_url: &str) -> Result<impl View> {
    let install_cmd = format!("curl -fsSL {public_url}/install.sh | bash");
    Ok(view! {
        shell(
            title: "Visual QA & Design Engineering",
            description: "Rust-native visual QA pipeline, AI UI skill, CDP capture runner, and Visual Stage Canvas panel.",
            active: Nav::Home,
            panel_url: panel_url,
            <section class="hero">
                <div class="container">
                    <div class="hero-center">
                        <p class="eyebrow">"Rust-Native Visual QA · CDP Captures + MCP"</p>
                        <h1>"Verify every state with captured proof."</h1>
                        <p class="hero-lead">"Static AST scans for 13 UI stacks, headless Chromium captures via CDP, and before/after visual triage in one Rust workspace."</p>
                        <div class="cta-row">
                            button(href: panel_url, primary: true, external: true, "Open Panel")
                            button(href: "/docs", "Read Docs")
                        </div>
                        <div class="cmd-bar">
                            <code>(&install_cmd)</code>
                            <button class="btn btn-sm" type="button" data-copy=(&install_cmd)>"Copy"</button>
                        </div>
                    </div>
                    hero_stage_preview()
                </div>
            </section>
            section(
                <div class="section-bar">
                    <h2>"Architecture & Natural Color Specimen"</h2>
                    <span class="mono muted section-note">"Zero JS frameworks · Topcoat SSR + Rust workspace"</span>
                </div>
                architecture_bento()
            )
        )
    })
}

/// Interactive Visual Stage Canvas preview embedded inside the hero.
#[component]
async fn hero_stage_preview(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <div class="app-frame" aria-label="Interactive Rupicola Visual Stage Canvas preview">
            <div class="frame-chrome">
                <div class="frame-dots" aria-hidden="true"><span></span><span></span><span></span></div>
                <span class="mono muted frame-meta">"rupicola-panel · Visual Stage Canvas"</span>
                <span class="pill pill-ok">"CDP Capture + Split Slider"</span>
            </div>
            <div class="frame-body">
                <div class="frame-canvas">
                    <div class="diff-box" id="hero-diff" data-vp="1440">
                        <div class="diff-head">
                            <span class="mono diff-stage-title" id="hero-stage-title">"settings-popover · 1440×900 · dark"</span>
                            <span class="pill pill-crest">"1.4% diff px"</span>
                        </div>
                        <div class="diff-canvas">
                            <div class="diff-side diff-before">
                                <span class="pill">"BEFORE · rev 710b"</span>
                                <div class="diff-sample-card">
                                    <strong>"Focus lost on Escape"</strong>
                                    <p class="mono diff-sample-note">"3px misaligned · low contrast"</p>
                                </div>
                            </div>
                            <div class="diff-side diff-after">
                                <span class="pill pill-ok">"AFTER · rev a91f02"</span>
                                <div class="diff-sample-card">
                                    <strong>"Focus #C24D1A + 1px grid"</strong>
                                    <p class="diff-sample-note">"Zero overflow · 78 Lc APCA"</p>
                                </div>
                            </div>
                            <div class="diff-pixel-mask"></div>
                            <div class="diff-line"></div>
                            <div class="diff-knob" aria-hidden="true">"↔"</div>
                            <input class="diff-range" data-hero-split="" type="range" min="12" max="88" value="50" aria-label="Before and after split slider">
                        </div>
                    </div>
                    <div class="stage-hud" role="toolbar" aria-label="Preview controls">
                        <button class="btn btn-sm btn-primary" type="button" data-hero-vp="1440">"1440px"</button>
                        <button class="btn btn-sm" type="button" data-hero-vp="768">"768px"</button>
                        <button class="btn btn-sm" type="button" data-hero-vp="320">"320px"</button>
                        <button class="btn btn-sm" type="button" data-hero-diff="">"Pixel Diff"</button>
                    </div>
                </div>
                <aside class="frame-rail" aria-label="Pipeline views">
                    <span class="mono muted frame-rail-label">"Pipeline Stage"</span>
                    <button class="frame-tab is-active" type="button" data-hero-tab="" data-hero-title="settings-popover · 1440×900 · dark">
                        <span>"01 Captures"</span><span class="mono">"5"</span>
                    </button>
                    <button class="frame-tab" type="button" data-hero-tab="" data-hero-title="PROB-14 · Escape focus · /settings">
                        <span>"02 Problems"</span><span class="mono">"3"</span>
                    </button>
                    <button class="frame-tab" type="button" data-hero-tab="" data-hero-title="FIX-09 · Verified Pixel Diff">
                        <span>"03 Fixes"</span><span class="mono">"4"</span>
                    </button>
                    <button class="frame-tab" type="button" data-hero-tab="" data-hero-title="Design Norms · #C24D1A Crest">
                        <span>"04 Design"</span><span class="mono">"6"</span>
                    </button>
                    <div class="muted frame-rail-hint">
                        "Drag the split handle or switch viewports."
                    </div>
                </aside>
            </div>
        </div>
    })
}

/// Technical architecture cards showcasing CDP captures, calibrated Rupicola palette, and supported stacks.
#[component]
async fn architecture_bento(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <div class="bento">
            <article class="b-cell c-7">
                <span class="pill pill-crest">"rupicola-protocol (CDP) → artifact_read"</span>
                <h3>"Headless captures & content-addressed bundles"</h3>
                <p class="muted b-copy">"Run multi-step Chromium captures over CDP via rupicola-protocol and publish SHA-256 bundles straight to the panel or inspect them over MCP."</p>
                code_block(label: "CLI Discovery & Publish", code: "rupicola scan . --json\nrupicola publish ./artifacts/capture-run")
            </article>

            <article class="b-cell c-5">
                <div class="bird-specimen">
                    <img src="/rupicola-bird.jpg" alt="Guianan cock-of-the-rock (Rupicola rupicola) showing deep orange crest and dark wings" loading="lazy">
                    <div class="bird-specimen-copy">
                        <h3>"Rupicola rupicola palette"</h3>
                        <p class="muted b-copy">"Deep crest orange calibrated in OKLCH over warm obsidian and parchment neutrals."</p>
                        <div class="bird-swatches">
                            <span class="swatch-chip"><i class="swatch-crest"></i>"#C24D1A"</span>
                            <span class="swatch-chip"><i class="swatch-copper"></i>"#F8A16B"</span>
                            <span class="swatch-chip"><i class="swatch-obsidian"></i>"#0E0D0C"</span>
                            <span class="swatch-chip"><i class="swatch-parchment"></i>"#F7F5F2"</span>
                        </div>
                    </div>
                </div>
            </article>

            <article class="b-cell c-4">
                <h3>"Offline Discovery & Scan"</h3>
                <p class="muted b-copy">"Read manifests and UI syntax with zero build script execution or model keys."</p>
                <code class="b-cmd">"rupicola discover . --json"</code>
                <div class="cta-row cta-row-start">
                    button(href: RELEASES_URL, external: true, "Releases")
                    button(href: "/architecture", "Architecture")
                </div>
            </article>

            <article class="b-cell c-8">
                <h3>"13 Rust & Web UI Stacks"</h3>
                <p class="muted b-copy">"Matched modules for web SSR/WASM, desktop GPU/software toolkits, and webviews."</p>
                <div class="tag-row">
                    framework_tags()
                </div>
            </article>
        </div>
    })
}
