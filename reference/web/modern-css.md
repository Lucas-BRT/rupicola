# Modern web platform features

Shared adaptation rules: [common](../common.md). For web frontends and for web UIs inside desktop shells ([webview](../environments/webview.md)). Movement tactics that use these features are in the [motion library](../motion/transitions.md).

Status below is a snapshot from 2026-11-18 compiled from MDN, web.dev, Chrome for Developers and Baseline trackers. Sources disagree on some dates and versions. Treat every status as something to confirm against MDN or the Baseline status page for the browsers the project supports before relying on it.

## Adoption rules

1. Baseline status decides the default. Widely available: use freely. Newly available: use with a feature check or a graceful fallback. Limited availability or experimental: use only as a progressive enhancement, never for essential function.
2. Read the project's support policy first (`browser_support: baseline-widely | baseline-newly | latest` in `.ferris-atelier/context.md`, or a policy in AGENTS.md or CLAUDE.md). With no policy, assume `baseline-widely` plus enhancements behind `@supports`.
3. Progressive enhancement means the experience is complete without the feature. A browser that ignores it must still do the task.
4. A new feature has to beat the plain alternative for this task. Do not use a feature because it is new, interesting or impressive; use it because it removes JavaScript, fixes a real problem or improves a measurable quality. Prefer one or two such features per page, not a showcase.
5. Webviews are pinned by the OS, not the user: Tauri and Dioxus desktop use WebKitGTK on Linux, WebView2 on Windows and WKWebView on macOS, and these lag or differ from desktop browsers. Check the minimum engine of each target before using a newly available feature.
6. Reduced motion, contrast and keyboard behavior still apply: see [accessibility](../ux/accessibility.md) and [motion principles](../ux/motion.md).
7. Detect, do not sniff: `@supports (property: value)`, `@supports selector(...)`, `CSS.supports()` and `'x' in element` checks.

## Pages and navigation

### Cross-document view transitions (multi-page sites, no SPA)
Status: Chrome and Edge 126, Safari 18.2; not in Firefox; limited availability. Same-document transitions are newly available since October 2025.
Use when: a multi-page or server-rendered site has navigations where continuity helps (list to detail, same header and a changing body). Do: opt both pages in with `@view-transition { navigation: auto; }` (same origin only), name the shared elements with `view-transition-name` (unique per page; `view-transition-class` styles groups), wrap the opt-in in `@media (prefers-reduced-motion: no-preference)`, keep the animation short, and add `pageswap` and `pagereveal` handlers only to choose a direction or name the clicked card (guard with `if (!event.viewTransition) return`). Use `rel="expect"` with `blocking="render"` only for above-the-fold content. Unsupported browsers do a normal instant navigation.
Don't: animate every link, forms, search and frequent navigations; block render on slow or deep content; rely on it for meaning; expect it on cross-origin, reload or back-forward cache restores; forget the transition times out if the new page cannot render quickly.

### Same-document view transitions
Status: newly available (Baseline since 14 October 2025). Use when: a client-side state swap should morph or crossfade (`document.startViewTransition`). Do: feature-detect, keep the DOM update small and fast, name only the elements that should morph. Don't: use it for trivial changes, long updates, or when a plain CSS transition does the job.

### Element-scoped and two-phase view transitions
Status: Chrome 147 for element-scoped, two-phase experimental. Use when: only a subtree should transition while the page stays interactive. Don't: ship without a fallback; skip for now in production.

## Scroll

### Scroll-driven animations
Status: newly available since 27 October 2026 (Firefox 159, Chrome and Edge 115, Safari 26). Use when: progress bars, reveal-on-scroll, sticky header shrink, parallax on marketing surfaces. Do: `animation-timeline: scroll()` or `view()` with `animation-range`; write `animation-timeline` after the `animation` shorthand (the shorthand resets it); Firefox needs a non-zero `animation-duration`; gate with `@supports (animation-timeline: scroll())`; animate `transform` and `opacity`; disable movement under reduced motion. Don't: hide essential content until it scrolls in; use it for long reading text; stack many scroll effects on one page.

### Scroll-triggered animations
Status: `timeline-trigger` and `animation-trigger` in Chrome and Edge 146 only. Use when: a one-shot animation should start when an element enters view. Do: treat as enhancement, keep an IntersectionObserver fallback or a static state. Don't: depend on it for visibility of content.

### Scroll-state container queries
Status: Chromium. Use when: style a header when it is stuck, an item when it is snapped, or a container when it is scrollable. Do: `container-type: scroll-state` with `@container scroll-state(...)`. Don't: make function depend on it.

### Native carousels: `::scroll-marker`, `::scroll-button()`
Status: Chrome and Edge 135+. Use when: a carousel needs working markers and buttons with no JavaScript. Do: guard with `@supports selector(::scroll-marker)`, keep a normal scrollable row as the fallback, keep keyboard and screen-reader behavior. Don't: hide content that is reachable only through the carousel controls.

### Scroll-spy: `scroll-target-group`, `:target-current`
Status: Chromium. Use when: a table of contents should follow the section in view. Don't: rely on it where `aria-current` must be correct everywhere.

## Layout and sizing

### Anchor positioning
Status: newly available since 13 January 2026 (Firefox 147; Chrome and Edge 125, Safari 26); sources differ on Safari and Chrome details. Use when: tooltips, popovers, menus and dropdowns that must sit next to a trigger and flip at edges. Do: `anchor-name` on the trigger, `position-anchor`, `position-area` and `position-try-fallbacks` on the popover; set `position-anchor` explicitly; combine with the Popover API. Don't: use it where a static layout works, or assume engines agree on edge cases without testing.

### `interpolate-size` and `calc-size()`
Status: limited availability (Chromium). Use when: animating to or from `height: auto` or intrinsic sizes. Do: `interpolate-size: allow-keywords` as an enhancement; the fallback is an instant change. Don't: require it for accordions to work.

### `field-sizing: content`
Status: newly available in 2026. Use when: textareas and inputs should grow with content. Do: keep a sensible min and max. Don't: use it on fixed-width form designs.

### `sibling-index()` and `sibling-count()`
Status: newly available since 18 August 2026 (Chrome and Edge 138, Safari 26.2, Firefox 154). Use when: staggered delays or positions without nth-child rules. Do: cap the delay so long lists do not wait; see the stagger rules in [choreography](../motion/choreography.md). Don't: stagger long lists on every render.

### Container queries and style queries
Size container queries are widely available. Style queries (`@container style(...)`) are an Interop 2026 area: check support. Use when: a component adapts to its own space. Don't: use viewport media queries for component-level changes, or use style queries as the only path.

### `shape()`, `border-shape`, `corner-shape`
Status: `shape()` newly available; `border-shape` and `corner-shape` (bevel, scoop, notch, squircle) experimental in Chromium. Use when: non-rectangular clipping or distinctive corners that are decorative. Do: provide a plain `border-radius` fallback. Don't: use unusual shapes where contrast of borders, focus rings or touch targets would suffer, or as a substitute for a design system.

### Gap decorations (`column-rule`, `row-rule` for grid and flex)
Status: Chrome and Edge 149 only. Use when: dividers between flex and grid items. Don't: depend on it; borders on items work everywhere.

### `text-wrap: balance` and `pretty`
`balance` is becoming widely available; `pretty` works in Chrome, Edge and Safari 26 and is ignored elsewhere. Use when: headings (`balance`) and body paragraphs (`pretty`). Don't: apply `balance` to long paragraphs (it is limited to a few lines and costs layout time).

### `text-box` (`text-box-trim`, `text-box-edge`)
Status: newly available in 2026. Use when: optical vertical alignment of text in buttons and badges. Don't: use it on multi-line body text without checking descenders.

## Overlays, inputs and components

### Popover API
Status: newly available since January 2025. Use when: menus, tooltips-with-content, toasts, non-modal overlays. Do: `popover` with `popovertarget`, which brings top layer, light dismiss and focus behavior. Don't: build a custom overlay with z-index.

### Invoker commands: `commandfor` and `command`
Status: newly available (Baseline since December 2025; Chrome 135, Firefox 144, Safari 26.2). Use when: buttons that open or close dialogs and popovers without JavaScript (`show-modal`, `close`, `toggle-popover`). Do: add a script fallback if older Safari matters. Don't: use it for logic beyond opening and closing.

### Dialog `closedby`
Status: not Baseline (Chrome 134, Firefox 141, Safari preview). Use when: light dismiss of a dialog (`closedby="any"`). Do: feature-detect and keep an explicit Close button. Don't: make outside click the only way to close.

### Interest invokers: `interestfor`, `popover="hint"`
Status: Chromium only. Use when: hover or focus previews. Do: feature-detect each separately; keep the content reachable by click or focus. Don't: use for essential information or on touch-only surfaces.

### Customizable select: `appearance: base-select`
Status: Chrome and Edge 135, Safari 27; not in stable Firefox; not Baseline. Use when: a select needs rich options (icons, two-line items). Do: wrap styling in `@supports (appearance: base-select)` so the native control is the fallback. Don't: restyle selects that work well natively, or break keyboard and screen-reader behavior.

### `::details-content` and exclusive accordions
`::details-content` is newly available (2025). Use when: animating or styling the opened area of `<details>`; group with the `name` attribute for exclusive accordions. Don't: replace `<details>` with JavaScript widgets.

## Color, logic and typography

### `contrast-color()`
Status: newly available since April 2026 (Chrome and Edge 147, Firefox 146, Safari 26). Use when: text on a dynamic or user-chosen background. Do: remember it returns only black or white and verify the ratio meets the target. Don't: treat it as a replacement for designed palettes or for checking non-text contrast.

### `light-dark()`
Colors are widely available; image values are newly available (September 2026). Use when: theme colors from one declaration. Do: set `color-scheme`. Don't: forget forced-colors and high-contrast modes.

### `if()`, `@function`, `random()`
Status: `if()` Chrome 137 and `@function` mostly Chromium; limited availability. `random()` unconfirmed. Use when: enhancement only. Do: provide a plain fallback value. Don't: build core styling on them.

### Advanced `attr()`
Status: Firefox 155 shipped (September 2026); an Interop 2026 area. Use when: reading typed attribute values in CSS. Don't: rely on it for layout without checking each target.

### `<meta name="text-scale">`, `text-fit`
Status: new and experimental. Use when: you want text to follow OS text-size settings or fit width. Don't: ship without testing at large text sizes.

## Animation helpers

### `@starting-style`, `transition-behavior: allow-discrete`
Newly available since 2024. Use when: entry and exit transitions for popovers, dialogs, `display` changes. Do: pair with the Popover API and dialogs. Don't: leave exit transitions that delay closing or ignore reduced motion.

### `linear()` easing
Widely available. Use when: spring and bounce curves in CSS. Don't: use a bounce for serious or repeated UI actions.

## DOM and platform

### `moveBefore()`
Status: recent, Chromium first. Use when: reparenting nodes without resetting playing media or focused inputs. Don't: use without a fallback.

### HTML in canvas, `Element.pseudo()`
Origin trial and Chrome 149 only. Do not use in production.

## Sources

[Cross-document transitions (MDN)](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@view-transition), [Chrome: cross-document view transitions](https://developer.chrome.com/docs/web-platform/view-transitions/cross-document), [Chrome: what's new in web UI 2026](https://developer.chrome.com/blog/new-in-web-ui-io26), [web.dev: new to the web platform, October 2026](https://web.dev/blog/web-platform-10-2026), [Interop 2026](https://hacks.mozilla.org/2026/02/launching-interop-2026/), [modern-css.com Baseline list](https://modern-css.com/whats-new/baseline/), [contrast-color() (MDN)](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/color_value/contrast-color).
