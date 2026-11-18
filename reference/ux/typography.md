# Text legibility

Shared adaptation rules: [common](../common.md). The rules below are self-contained; do not fetch the source to apply them.

## Rules

1. Text must still work at 200% size and with user spacing overrides (WCAG 1.4.4, 1.4.12): no clipping, no overlap, no fixed-height text boxes.
2. Use relative or toolkit-scaled units so system text-size settings apply. Do not hard-code pixel sizes for body text in toolkits that offer a scale factor.
3. Heuristics (not standards): body text around 16 px or the platform default, line height near 1.4 to 1.6, line length roughly 45 to 75 characters. Wide screens should widen margins, not lines.
4. Limit the type scale to a few sizes with a clear hierarchy; use weight and size, not color alone, to show importance.
5. Prefer the platform UI font unless identity requires otherwise; verify that custom fonts cover every shipped locale (see localization).
6. Right-align numbers in tables with tabular figures so columns compare at a glance.

## Examples

- Bad: a text box with a fixed height of 20 px that clips at 200% text size. Good: height follows content, with a minimum.
- Bad: body text at 11 px in `#888888`. Good: the platform default size (or about 14 to 16 px) and a color that passes 4.5:1.
- Bad: a paragraph spanning 140 characters on a 4K window. Good: `max-width: 65ch` (or a clamp container) with margins growing instead.
- Bad: a table column of amounts in a proportional font, ragged. Good: right-aligned with tabular figures (`font-variant-numeric: tabular-nums` on the web).

## Check

Render the changed screen at 100% and 200% text size and at the narrowest supported window; look for clipped or overlapping text. Check the longest realistic string, not the placeholder.

[Source](https://www.w3.org/WAI/WCAG22/Understanding/resize-text.html), kept for audit.
