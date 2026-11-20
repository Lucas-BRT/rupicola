# UI and UX tactics library

Shared adaptation rules: [common](../common.md). Numeric thresholds and checks live in the [UX modules](../ux/index.md); movement has its own [motion library](../motion/index.md). This library is the catalog of tactics: what to reach for, when, and what to avoid. Load this index first, then at most two category files.

## Proactive protocol

The user does not need to know these tactics exist. While planning, building, improving or reviewing a UI:

1. Scan the screen or task for the situations below, and for states the request did not mention (empty, loading, error, offline, disabled, long text, large data, narrow window, right-to-left, keyboard only).
2. Pick the few tactics with the highest user value; do not apply a tactic because it exists.
3. Act by workflow. Build: implement what the feature's own states and inputs require (labels, validation, empty, loading and error states, keyboard path). Improve: propose a ranked list with where and why, and apply it when polish or usability is the request or the user agrees. Review: report a missing tactic as a defect only when it causes a task failure, a false state or an accessibility barrier; otherwise as an opportunity. Plan: include the chosen tactics in the plan.
4. Read `proactivity: off | propose | apply` from `.rupicola/context.md` when it exists and obey it. Respect the project: reuse its components, tokens, tone and platform conventions; preserve brand decisions; do not expand scope silently. Say in plain words what was added and why.
5. Each tactic ships with how to verify it. Unavailable checks stay unverified.

## Situation to tactics

| Situation | Tactics | File |
|---|---|---|
| Arranging a screen, window or panel; narrow and wide windows; density | grouping, grid, adaptive panes, max width, density modes | [layout-structure](layout-structure.md) |
| Establishing look and consistency: type, color, surfaces, themes | type scale, semantic color roles, dark mode, elevation, tokens | [visual-system](visual-system.md) |
| Choosing a look; avoiding a generic result | design direction, anti-patterns, polish | [design-direction](design-direction.md) |
| Forms, settings, search, text and number entry, commands | labels, validation timing, defaults, steppers, command palette | [forms-input](forms-input.md) |
| Moving between areas; finding things; first run | sidebar, tabs, breadcrumbs, search, menus, onboarding | [navigation-ia](navigation-ia.md) |
| Anything can be empty, slow, failing, offline or irreversible | state set, notifications, undo, recovery, safe defaults | [states-feedback](states-feedback.md) |
| Tables, lists, cards, charts, dashboards, filters | table, list, chart choice, filters, bulk actions | [data-display](data-display.md) |
| Words on screen: buttons, errors, empty states, prompts | verbs, error anatomy, confirmation text, tone | [microcopy](microcopy.md) |
| Keyboard, screen reader, low vision, cognitive load | semantics, focus management, announcements, forgiving input | [accessibility](accessibility.md) |
| More than one language, region or direction | strings, plurals, bidi, formats, expansion | [localization](localization.md) |
| Reproducing what the web now does declaratively (anchored popovers, light dismiss, invoker commands, rich selects, auto-growing inputs, balanced headings, contrast-color) in a native toolkit | anchored popover, layered popover, command ids, rich select, helpers | [native-components](native-components.md) |
| Looking native on Linux, Windows, macOS or in a webview | header bars, menus, shortcuts, theme following, system integration | [platform-conventions](platform-conventions.md) |

## Purpose to tactics

Orient: grouping, wayfinding, breadcrumbs, current-location highlight. Decide: one primary action, defaults, comparison tables, progressive disclosure. Enter data: labels, validation timing, autocomplete, masks. Trust: undo, preview, confirmation naming the action, permission rationale. Recover: error anatomy, retry, draft saving. Scan: hierarchy, alignment, tabular figures, density. Include: keyboard path, accessible names, scaling, plain language, localization.

## What not to add

A tactic that is not needed for the task, a pattern that fights the platform, decoration that costs legibility, and anything the project's context file turns off.
