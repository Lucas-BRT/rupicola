# Meaning and contrast

Shared adaptation rules: [common](../common.md). The rules below are self-contained; do not fetch the source to apply them.

## Rules

1. Normal text needs contrast of at least 4.5:1 against its background; large text (24 px regular or 18.66 px bold and up) needs 3:1 (WCAG 1.4.3).
2. Control boundaries, icons that carry meaning, charts and focus indicators need 3:1 against adjacent colors (WCAG 1.4.11). Disabled controls are exempt.
3. Never encode state by hue alone (WCAG 1.4.1). Pair selected, error, warning and success with a text label, icon or shape.
4. Check every theme you ship (light, dark, high contrast) separately; a passing pair in one theme says nothing about another.
5. Use semantic roles (`error`, `surface`, `on-surface`) from the toolkit theme instead of literal hex values, so themes and system contrast settings still apply.

## Examples

- Bad: hint text `#999999` on white, about 2.85:1. Good: `#595959`, about 7:1 (AA needs 4.5:1; `#777777` is 4.48:1 and still fails).
- Bad: an invalid field shown only by a red border. Good: red border plus an error icon and the text "Enter a valid email", with `aria-invalid="true"` on the web.
- Bad: a literal `Color::from_rgb8(0xE5, 0x48, 0x4D)` in each widget. Good: the toolkit's theme role (libcosmic theme, iced `Theme::palette()`, egui `ui.visuals()`), so dark and high-contrast themes adapt.
- Bad: a chart distinguishing two series by red and green only. Good: different markers or dash patterns plus direct labels.

## Check

Contrast ratio is (L1 + 0.05) / (L2 + 0.05) with relative luminance from linear sRGB (0.2126 R + 0.7152 G + 0.0722 B), L1 the lighter. Compute it for each foreground/background pair in each theme and list the failures with their ratios. A screenshot is not a contrast check.

## Adapt

Brand colors stay if they pass or if the brand pair is used only for decoration. A hue alone is not a confirmed defect when a text or shape signal is present.

[Source](https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html), kept for audit; WCAG 2.2 AA is the reference level.
