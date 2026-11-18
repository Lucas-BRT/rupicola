# Input and keyboard

Shared adaptation rules: [common](../common.md). The rules below are self-contained; do not fetch the source to apply them.

## Rules

1. Every action reachable by pointer must be reachable by keyboard, without a trap (WCAG 2.1.1, 2.1.2). Tab order follows reading order (2.4.3).
2. Keyboard focus is always visible and never hidden behind overlays or sticky bars (2.4.7, 2.4.11); the indicator needs 3:1 contrast.
3. Pointer targets are at least 24 x 24 CSS px, or have equivalent spacing (WCAG 2.5.8). Platform guidance is larger for touch: about 44 pt (Apple) and 48 dp (Material). Use the larger value on touch-first surfaces.
4. Single-character shortcuts must be remappable or disabled unless they only work while the control has focus (2.1.4). Prefer modifier chords for global shortcuts.
5. Anything done by dragging needs a single-pointer alternative such as buttons or a menu (2.5.7).
6. Acknowledge input immediately, even when the operation takes longer; keep entered text and focus across validation errors and async completion.

## Examples

- Bad: a 16 x 16 px icon button with a 2 px gap. Good: at least 24 x 24 px, or keep the 16 px glyph inside a 32 px hit area (Slint: a 32 px `Rectangle` holding the `TouchArea`).
- Bad: `outline: none` with no replacement. Good: `:focus-visible { outline: 2px solid; outline-offset: 2px; }` with at least 3:1 contrast.
- Bad: pressing `D` anywhere deletes the selected item. Good: `Delete` only while the list has focus, or a modifier chord, plus an undo.
- Bad: a drag handle as the only way to reorder. Good: also "Move up" and "Move down" actions in the item menu.

## Check

Operate the changed flow with the keyboard only, in tab order and in reverse, then with a screen reader if one is available. Measure the smallest interactive target in the changed screen. Report which of these could not be run.

## Adapt

Desktop toolkits implement focus, IME and shortcuts through their own mechanisms; do not prescribe DOM attributes outside DOM renderers. Follow the OS convention for shortcuts before inventing new ones.

[Source](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html), kept for audit.
