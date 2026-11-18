# Transitions: appear, disappear, switch

Shared rules: [common](../common.md), [index](index.md). Easing by intent: entering uses ease-out (decelerates into place), leaving uses ease-in (accelerates away), moving between two on-screen positions uses ease-in-out, progress and loops use linear. Exit is usually faster than enter (about 70 to 80%). Duration scales with distance and size, not a single number: small property changes feel right quickly, large surfaces need longer, and anything near full screen should still stay well under a second.

### Fade
Communicates: presence without direction. Use when: tooltips, toasts, overlays, content swaps where spatial change is noise. Recipe: opacity 0 to 1. Cheapest and safest; the default reduced-motion replacement for movement. Avoid: fading text that the user is trying to read or edit.

### Slide (translate)
Communicates: where something comes from or goes to. Use when: drawers, side panels, snackbars, sheets, wizard steps. Recipe: translate from the edge it belongs to, 8 to 24 px for in-place hints and 100% for edge panels, combined with fade. Direction is meaning: forward moves left, back moves right (mirror in right-to-left). Avoid: sliding large areas for small updates.

### Scale from origin
Communicates: this came from that control. Use when: popovers, menus, dialogs opened from a trigger. Recipe: scale from about 0.95 to 1 with `transform-origin` at the trigger, plus fade; never start from 0 (feels like a pop). Avoid: scaling text-heavy content far above 1.

### Clip or mask reveal
Communicates: content uncovered rather than moved. Use when: accordions, expanding rows, image reveals, wipes, iris from a click point. Recipe: animate a clip rectangle or circle (web `clip-path`, `mask`; native clip regions); avoids reflow jitter that animating `height` can cause. Avoid: animating layout height on long lists.

### Collapse and expand
Communicates: space opened or closed. Use when: accordions, disclosure, inline errors. Recipe: animate the container size and fade content slightly later; measure the target size instead of animating to `auto`; where the toolkit has a revealer widget, use it. Avoid: animating every sibling's position per frame on large pages (use FLIP).

### Crossfade
Communicates: same place, new content. Use when: tab content with no order, image swap, state swap (loading to loaded). Recipe: stack both states, fade out one while fading in the other, keep the container size stable. Avoid: crossfading two dense text blocks; slide or cut instead.

### Directional slide between views
Communicates: order and hierarchy. Use when: push and pop navigation, tab content by index, carousel. Recipe: new view enters from the side matching its position, old one leaves opposite by a shorter distance (parallax of about 20 to 30%) so it feels layered. Keep it interruptible. Avoid: sliding between peer tabs that have no order.

### Shared element and container transform
Page-to-page versions on multi-page sites use cross-document view transitions; see their status, opt-in rules and when not to use them in [modern web features](../web/modern-css.md).
Communicates: this detail is the same object as that card. Use when: list or grid item to detail, thumbnail to full image, FAB to sheet. Recipe: animate the element's rectangle (position, size, corner radius) from source to target while the rest crossfades; on the web use the View Transitions API with matching `view-transition-name`s (check browser support), in native toolkits animate between two measured rects. Avoid: when source and target are not visibly the same thing, or when it blocks input.

### Color, border and elevation change
Communicates: state (hover, selected, focus). Use when: any interactive control. Recipe: animate color, border color and shadow over a short time; keep contrast thresholds (see the color module) at every frame that persists. Avoid: relying on the color change alone.

### Shape and icon morph
Communicates: one control became another. Use when: menu to close, play to pause, expand arrow. Recipe: interpolate matching path points, or rotate and crossfade a pair of glyphs. Avoid: morphing paths with different point counts without preparing them.

### Path draw
Communicates: progress or completion. Use when: check marks, progress rings, signatures, onboarding lines. Recipe: animate the stroke dash offset from the path length to 0. Avoid: using it as decoration on every icon.

### Blur and filter
Communicates: depth and focus. Use when: modal backdrops, background dimming, glass surfaces. Recipe: fade a backdrop blur in with the surface; blur is expensive, so animate opacity, not the blur radius, and cap the radius. Avoid: blur on low-power targets or large animated areas.

### 3D flip and depth
Communicates: two sides of one object. Use when: card flip, settings back face. Recipe: rotate about one axis with perspective, swap faces at 90 degrees, hide back faces. High cost and high vestibular risk. Avoid: by default; use only for `expressive` and give a crossfade fallback.
