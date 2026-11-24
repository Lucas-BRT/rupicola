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
            description: "Rust-native visual QA pipeline, AI UI skill, server capture runner, and Visual Stage Canvas panel.",
            active: Nav::Home,
            panel_url: panel_url,
            <section class="hero">
                <div class="container">
                    <div class="hero-center">
                        <span class="pill pill-crest">"Rust-Native Visual QA · Server Capture Runner + MCP"</span>
                        <h1>"Verify every state with captured proof."</h1>
                        <p class="hero-lead">"Static AST inspection for 13 UI stacks, server-side Chromium captures via " <code>"capture_run"</code> ", and pixel-accurate Before/After triage in one lean Rust workspace."</p>
                        <div class="cta-row">
                            button(href: panel_url, primary: true, external: true, "Open Panel")
                            button(href: "/install", "Install CLI & Skill")
                            button(href: "/docs", "Read Docs")
                            <div class="cmd-bar">
                                <code>(&install_cmd)</code>
                                <button class="btn btn-sm" type="button" data-copy=(&install_cmd)>"Copy"</button>
                            </div>
                        </div>
                    </div>
                    hero_stage_preview()
                </div>
            </section>
            section(
                <div style="display:flex;justify-content:space-between;align-items:baseline;flex-wrap:wrap;gap:0.75rem;margin-bottom:1rem">
                    <h2>"Architecture & Natural Color Specimen"</h2>
                    <span class="mono muted" style="font-size:12px">"Zero JS frameworks · Topcoat SSR + Rust workspace"</span>
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
                <span class="mono muted" style="font-size:11px">"rupicola-panel · Visual Stage Canvas"</span>
                <span class="pill pill-ok">"capture_run + Split Slider"</span>
            </div>
            <div class="frame-body">
                <div class="frame-canvas">
                    <div class="diff-box" id="hero-diff" data-vp="1440" style="--pos:56%">
                        <div style="display:flex;justify-content:space-between;align-items:center;padding:6px 10px;background:var(--rail);border-bottom:1px solid var(--line)">
                            <span class="mono" id="hero-stage-title" style="font-size:11px">"settings-popover · 1440×900 · dark"</span>
                            <span class="pill pill-crest">"1.4% diff px"</span>
                        </div>
                        <div class="diff-canvas">
                            <div class="diff-side diff-before">
                                <span class="pill" style="background:#18181b;border-color:#27272a;color:#e4e4e7;width:fit-content">"BEFORE · rev 710b"</span>
                                <div style="border:1px solid #27272a;padding:10px;border-radius:4px;background:#111114;font-size:12px">
                                    <strong>"Focus lost on Escape"</strong>
                                    <p class="mono" style="color:#a1a1aa;margin-top:4px;font-size:11px">"3px misaligned column · low placeholder contrast"</p>
                                </div>
                            </div>
                            <div class="diff-side diff-after">
                                <span class="pill pill-ok" style="width:fit-content">"AFTER · rev a91f02"</span>
                                <div style="border:1px solid var(--line);padding:10px;border-radius:4px;background:var(--rail);font-size:12px">
                                    <strong>"Focus ring #FB6803 + 1px hairline grid"</strong>
                                    <p class="muted" style="margin-top:4px;font-size:11px">"Zero overflow at 320px · contrast 14.2:1 · tabular numbers"</p>
                                </div>
                            </div>
                            <div class="diff-pixel-mask"></div>
                            <div class="diff-line"></div>
                            <div class="diff-knob" aria-hidden="true">"↔"</div>
                            <input class="diff-range" data-hero-split="" type="range" min="12" max="88" value="56" aria-label="Before and after split slider">
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
                    <span class="mono muted" style="font-size:11px;text-transform:uppercase">"Pipeline Stage"</span>
                    <button class="frame-tab is-active" type="button" data-hero-tab="" data-hero-title="settings-popover · 1440×900 · dark">
                        <span>"01 Captures"</span><span class="mono">"5"</span>
                    </button>
                    <button class="frame-tab" type="button" data-hero-tab="" data-hero-title="PROB-14 · Escape focus · /settings">
                        <span>"02 Problems"</span><span class="mono">"3"</span>
                    </button>
                    <button class="frame-tab" type="button" data-hero-tab="" data-hero-title="FIX-09 · Verified Pixel Diff">
                        <span>"03 Fixes"</span><span class="mono">"4"</span>
                    </button>
                    <button class="frame-tab" type="button" data-hero-tab="" data-hero-title="Design Norms · #FB6803 Crest">
                        <span>"04 Design"</span><span class="mono">"6"</span>
                    </button>
                    <div style="border-top:1px solid var(--line);padding-top:8px;font-size:11.5px" class="muted">
                        "Drag the split handle or switch viewports."
                    </div>
                </aside>
            </div>
        </div>
    })
}

/// Technical 1px bento grid showcasing `capture_run`, sampled Rupicola palette, and supported stacks.
#[component]
async fn architecture_bento(cx: &Cx) -> Result<impl View> {
    Ok(view! {
        <div class="bento">
            <article class="b-cell c-7">
                <span class="pill pill-crest" style="width:fit-content">"MCP capture_run → artifact_read"</span>
                <h3>"Server-side headless captures"</h3>
                <p class="muted">"Run multi-step Playwright/Chromium captures straight from MCP, CLI or the panel; artifacts are content-addressed with SHA-256 for immediate inspection."</p>
                code_block(label: "CLI Capture Run", code: "rupicola captures run https://preview.example/settings \\\n  --scenario popover --steps-file steps.json --json")
            </article>

            <article class="b-cell c-5">
                <div class="bird-specimen">
                    <img src="/rupicola-bird.jpg" alt="Guianan cock-of-the-rock (Rupicola rupicola) showing bright orange crest and dark wings" loading="lazy">
                    <div style="display:grid;gap:0.45rem">
                        <h3>"Rupicola rupicola palette"</h3>
                        <p class="muted" style="font-size:0.8rem">"Sampled directly from Wikimedia plumage pixels over neutral cold graphite & porcelain."</p>
                        <div class="bird-swatches">
                            <span class="swatch-chip"><i style="background:#fb6803"></i>"#FB6803"</span>
                            <span class="swatch-chip"><i style="background:#d84803"></i>"#D84803"</span>
                            <span class="swatch-chip"><i style="background:#09090b"></i>"#09090B"</span>
                            <span class="swatch-chip"><i style="background:#fafafa"></i>"#FAFAFA"</span>
                        </div>
                    </div>
                </div>
            </article>

            <article class="b-cell c-4">
                <h3>"Offline Discovery & Scan"</h3>
                <p class="muted" style="font-size:0.82rem">"Read manifests and UI syntax with zero build script execution or model keys."</p>
                <code style="font-size:0.78rem;color:var(--copper)">"rupicola discover . --json"</code>
                <div class="cta-row" style="justify-content:start;margin-top:auto">
                    button(href: RELEASES_URL, external: true, "Releases")
                    button(href: "/architecture", "Architecture")
                </div>
            </article>

            <article class="b-cell c-8">
                <h3>"13 Rust & Web UI Stacks"</h3>
                <p class="muted" style="font-size:0.82rem">"Matched modules for web SSR/WASM, desktop GPU/software toolkits, and webviews."</p>
                <div class="tag-row">
                    framework_tags()
                </div>
            </article>
        </div>
    })
}
