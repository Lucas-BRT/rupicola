# Layout and responsiveness

Shared adaptation rules: [common](../common.md). The rules are self-contained; do not fetch the source to apply them.

## Rules

1. Use a spacing scale (for example 4, 8, 12, 16, 24, 32 px) from theme tokens, not one-off values. Heuristic: multiples of 4 or 8 keep alignment predictable.
2. Let the layout engine size things. Prefer containers that fill, wrap or scroll over fixed widths and heights; fixed-height text boxes clip at larger text sizes.
3. Cap line length of reading content (about 45 to 75 characters) with a max-width container; wider windows add margin, not wider lines.
4. Content must reflow at narrow widths (equivalent to 320 CSS px) without two-axis scrolling, except for data tables, maps and code (WCAG 1.4.10). Define a minimum window size and test at it.
5. Scroll one region per axis at a time; avoid nested scrollers with the same direction.
6. Keep primary controls in stable places across states; loading and error states must not shift the whole layout.

## Examples

- egui: `ui.horizontal_wrapped(|ui| { ... })` lets a row of chips wrap; `ui.horizontal` clips them on a narrow window.
- libadwaita: wrap content in `adw::Clamp` so a form stays about 600 px wide on a maximized window; use breakpoints to move a sidebar into an overlay when narrow.
- Web or Leptos: `grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr))` instead of three fixed columns.
- Bad: a 320 px fixed-width sidebar plus content at a 600 px window leaves 280 px for content. Good: sidebar collapses to an overlay below about 720 px.

## Check

Reproduce failures with the actual content, then inspect the parent constraints and intrinsic size before changing child dimensions.

Resize to the minimum supported size and to a very wide window; trigger longest realistic text and 200% text scaling; confirm nothing clips, overlaps or scrolls in two directions.

## Adapt

Toolkits differ: immediate-mode UIs (egui) compute layout each frame and need explicit wrapping and scroll areas; retained toolkits (GTK4, Slint, Iced) expose constraint-based layouts. Use the project's own layout primitives. [Source](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html), kept for audit.
