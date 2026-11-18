# Icons

Shared adaptation rules: [common](../common.md). The rules are self-contained; do not fetch the source to apply them.

## Rules

1. Use an icon alone only when its meaning is near-universal (close, search, settings, add). Otherwise pair it with a visible text label.
2. Every icon-only control needs an accessible name and a tooltip with the same text; the tooltip is not the accessible name.
3. Keep one icon family: same stroke weight, corner style and optical size. Typical sizes are 16 and 24 px; do not scale bitmaps.
4. Prefer theme-aware vector icons that take the text color (on GNOME, symbolic icons with the `-symbolic` suffix) so dark mode and high contrast still work.
5. Icons that show direction (back, forward, send) mirror in right-to-left locales; clocks, logos and media play controls do not.
6. Icons that carry state (error, success) also change shape or text, not just color; give meaningful icons 3:1 contrast against their background (WCAG 1.4.11).

## Examples

- Bad: a bare floppy-disk glyph, no label, no tooltip. Good: "Save" label next to the glyph in a toolbar, or at least a tooltip and accessible name "Save".
- Slint: `accessible-role: button; accessible-label: "Undo";` on the clickable icon.
- gtk4-rs: `gtk::Button::from_icon_name("edit-undo-symbolic")` with `set_tooltip_text(Some("Undo"))` (illustrative; check the crate version).
- Bad: a PNG at 24 px stretched to 48 px. Good: an SVG or icon font rendered at 48 px.

## Check

List every icon-only control and confirm it has an accessible name; switch to dark and high-contrast themes and a right-to-left locale; confirm icons stay visible and arrows flip.

[Source](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html), kept for audit.
