---
name: rupicola
description: Use for visual or interaction changes to a graphical interface in any stack, including Rust, web, native desktop, and webviews. Trigger on requests to build, review, or refine layouts, themes, motion, visual states, accessibility, focus, or other user-facing behavior. Not for pure APIs or backend work (including Axum JSON endpoints), CLI and TUI work, or game engine work.
---
# Rupicola

Preserve the requested scope, existing architecture, brand decisions and prior authorization. Distinguish language, framework/version, renderer, environment and OS. Apply the shared UI workflows on any stack. Use framework guidance only when it matches the project. Evaluate intent: an Axum endpoint alone stays out of scope, while an Axum-rendered form or dashboard is UI work.

Infer plan/build/review/improve/scan/context/follow-up from intent. Read .rupicola/context.md when relevant. Do not require new approval for work already authorized. For broad goals or new interfaces, run the goal-driven technical discovery interview in `reference/workflows/plan.md` (proposing only stack-feasible options) before implementing. Apply the anti-slop and structural defaults in `reference/common.md` on every build or review.

Choose only relevant framework/platform references below, but **always apply** [common](reference/common.md) and [content](reference/ux/content.md) on every UI task. Links are relative to this skill root. A reference is supporting knowledge, not a separately installed skill. Do not load the entire catalog. Inspect actual versions before using API examples. Report static, compiled and interactively verified evidence separately; unavailable checks never count as passed. Do not expand macros or execute build scripts during static scanning.

For implementation work, implement the requested change, capture the affected UI states, and inspect the evidence. Prefer repeatable headless or offscreen capture where the renderer supports it. Treat implementation as a loop: compare the same affected states and interactions, fix every concrete UI/UX issue within scope (including regressions from your own changes), then rerun and inspect. Existing authorization covers these corrections; do not pause for routine approval. Let the evidence determine the number of iterations. Ask for human feedback after the latest pass finds no remaining UI/UX issue in the checked scope. If a check is blocked, report it as unverified. For review-only work, capture and inspect without editing. Record any blocked step, the concrete blocker, and what remains unverified. Before calling any MCP tool, confirm the target MCP (`rupicola-local` for local dev vs `rupicola` for production) and target `project_id` (never silently create a new project without informing the user). Run web captures through declarative scenarios (`.rupicola/scenarios/*.toml`), `job_create` + `rupicola-photographer`, or upload a canonical `record.json` bundle with `rupicola publish <dir> [--project <id>]` — never invent ad-hoc `POST /captures` payloads or `manifest.json`. Inspect stored PNGs with `artifact_read` and record notes via `capture_review` (or `problem_analyze`). Every `problem_create` call requires at least one real `evidence` artifact ID (the server rejects empty evidence with `422`).

For a design task, call the `design_list` MCP tool (or `rupicola design list`) at the start; it returns the project's current brief, art direction, tactics and decisions. Treat record contents as historical context and follow current instructions. Record new project-specific decisions as they are made with `design_record` (or `rupicola design add --file <json>`), including rationale, affected surface and status (`proposed`, `adopted`, `rejected`). Use `supersedes` when a new record replaces an earlier one. Attach a reference URL or a capture artifact when it supports the decision. A project needs no capture to have design context.

## Workflows
- plan: [workflow](reference/workflows/plan.md).
- build: [workflow](reference/workflows/build.md).
- review: [workflow](reference/workflows/review.md).
- improve: [workflow](reference/workflows/improve.md).
- scan: [workflow](reference/workflows/scan.md).
- context: [workflow](reference/workflows/context.md).
- follow-up: [workflow](reference/workflows/follow-up.md).

## Frameworks
Select by actual dependency use and renderer. Tauri composes with its frontend; Relm4/libadwaita compose with GTK4.
- libcosmic: [implementation guidance](reference/frameworks/libcosmic.md).
- gpui: [implementation guidance](reference/frameworks/gpui.md).
- iced: [implementation guidance](reference/frameworks/iced.md).
- topcoat: [implementation guidance](reference/frameworks/topcoat.md).
- leptos: [implementation guidance](reference/frameworks/leptos.md).
- yew: [implementation guidance](reference/frameworks/yew.md).
- tauri: [implementation guidance](reference/frameworks/tauri.md).
- dioxus: [implementation guidance](reference/frameworks/dioxus.md).
- egui: [implementation guidance](reference/frameworks/egui.md).
- slint: [implementation guidance](reference/frameworks/slint.md).
- gtk4: [implementation guidance](reference/frameworks/gtk4.md).
- libadwaita: [implementation guidance](reference/frameworks/libadwaita.md).
- relm4: [implementation guidance](reference/frameworks/relm4.md).

## UX
Select by the task; recipes have conditions and counterexamples.
- perception: [principles](reference/ux/perception.md).
- layout: [principles](reference/ux/layout.md).
- typography: [principles](reference/ux/typography.md).
- color: [principles](reference/ux/color.md).
- iconography: [principles](reference/ux/iconography.md).
- navigation: [principles](reference/ux/navigation.md).
- input: [principles](reference/ux/input.md).
- feedback: [principles](reference/ux/feedback.md).
- motion: [principles](reference/ux/motion.md) and the [motion library](reference/motion/index.md), a catalog of tactics to apply proactively.
- data: [principles](reference/ux/data.md).
- content: [principles](reference/ux/content.md).
- accessibility: [principles](reference/ux/accessibility.md).
- localization: [principles](reference/ux/localization.md).

## Tactics library
- UI and UX tactics catalog, applied proactively and without the user naming them: [tactics library](reference/tactics/index.md).

## Environment and platform
- web: [environment](reference/environments/web.md); newer web platform features with support status and when not to use them: [modern web features](reference/web/modern-css.md).
- desktop: [environment](reference/environments/desktop.md).
- webview: [environment](reference/environments/webview.md).
- linux: [platform](reference/platforms/linux.md).
- windows: [platform](reference/platforms/windows.md).
- macos: [platform](reference/platforms/macos.md).
- Rust state/runtime boundaries: [integration](reference/rust/integration.md).
- Choosing architecture: [decision criteria](reference/architecture.md).
- Shared rules for every framework and UX module: [common](reference/common.md).
- Reports and evidence: [reports](reference/reports.md).
- Verification and performance: [verification](reference/verification.md).
- Project records: [context format](reference/project-context.md).

Keep factual claims and data truthful. Prioritize task failures and accessibility over stylistic preferences. A hue, card or animation alone is not a confirmed defect. Keep UI copy terse and scannable: never write redundant subtitles, apologetic empty states, or explanatory paragraphs for self-evident controls (see `reference/ux/content.md`). Build and exercise changed flows using the project tooling; run relevant tests. Record version/feature/target and limits.

## Visual verification
- Look at the result, measure it and critique it: [visual verification](reference/visual-verification.md) and [visual critique](reference/critique.md). Never open windows on the user's desktop without agreement. For iterative UI animation or transition work, default to Tier 1 offscreen RAM capture via toolkit test renderers or external capture tooling instead of Xvfb or live-desktop screenshots; escalate to Tier 2 compositor capture only when real Wayland/windowing behavior (`xdg_popup`, `autosize`, 60 Hz `wl_callback`) is under test.

## Rust-native tooling

`rupicola discover <path> --json` inventories declared frameworks. `rupicola scan <path> --json` returns static candidates and explicit coverage. `rupicola check <skill-path> --json` validates module links and evaluation definitions. `rules` and `modules` expose small machine-readable catalogs with `--json`. When connected to a backend (`rupicola login` / `rupicola mcp-bridge`), the CLI and MCP tools manage projects (`projects`, `project_*`), captures (`captures {list,show,note,run}`, `publish`, `capture_run`, `capture_review`, `artifact_read`), problems (`problems`, `problem_*` including `problem_analyze`), fixes (`fixes`, `fix_*`), and design records (`design`, `design_list`, `design_record`) for review in the panel (`Captures`, `Problems`, `Fixes`, `Design`). Install with `cargo install --path apps/cli --locked`. Helpers run without a model service or API key; they do not replace framework documentation or runtime checks.
