---
name: ferris-atelier
description: Rust UI skill. Load it first whenever the user mentions a GUI, window, widget, view, form, layout, theme, dark mode, accessibility or keyboard focus in a Rust project, or names libcosmic, GPUI, Iced, Topcoat, Leptos, Yew, Tauri, Dioxus, egui, eframe, Slint, GTK4, libadwaita or Relm4. Use it to plan, build, review or improve the interface. Not for backend, CLI or TUI work (axum, clap, ratatui), game engines, or non-Rust UIs (React, Flutter, Qt).
---
# Ferris Atelier

Preserve the requested scope, existing architecture, brand decisions and prior authorization. Distinguish language, framework/version, renderer, environment and OS. Ordinary Rust backend work alone does not activate this skill.

Infer plan/build/review/improve/scan/context/follow-up from intent. Read .ferris-atelier/context.md when relevant. Do not require new approval for work already authorized.

Choose only relevant references below. Links are relative to this skill root. A reference is supporting knowledge, not a separately installed skill. Do not load the entire catalog. Inspect actual versions before using API examples. Report static, compiled and interactively verified evidence separately; unavailable checks never count as passed. Do not expand macros or execute build scripts during static scanning.

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

Keep factual claims and data truthful. Prioritize task failures and accessibility over stylistic preferences. A hue, card or animation alone is not a confirmed defect. Build and exercise changed flows using the project tooling; run relevant tests. Record version/feature/target and limits.

## Visual verification
- Look at the result, measure it and critique it: [visual verification](reference/visual-verification.md) and [visual critique](reference/critique.md). Never open windows on the user's desktop without agreement.

## Rust-native tooling

`ferris-atelier discover <path> --json` inventories declared frameworks. `ferris-atelier scan <path> --json` returns static candidates and explicit coverage. `ferris-atelier check <skill-path> --json` validates module links and evaluation definitions. `diff`, `frames`, `palette`, `sheet`, `matrix` and `audit` support visual verification: compare PNGs, check animation frames, extract a palette, build a contact sheet, run a capture command over a state matrix, and audit a JSON UI tree. `rules` and `modules` expose small machine-readable catalogs with `--json`. Install with `cargo install --path <skill-path> --locked`. Helpers run without a model service or API key; they do not replace framework documentation or runtime checks.
