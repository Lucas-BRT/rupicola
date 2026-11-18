# Native equivalents of modern web components and layout features

Shared rules: [common](../common.md), [index](index.md). Motion-related equivalents are in [native effects](../motion/native-effects.md). The web gained declarative overlays, rich selects, auto-sizing inputs and smarter typography. A native toolkit gives more control, so reproduce the behavior from state you own. Each card names the web feature, the toolkit-neutral recipe, the usual toolkit hooks (confirm against the pinned version) and when not to use it. Use a tactic only when the task needs it.

Compiled examples of the popover with flip, shift and light dismiss: [egui](../../examples/egui-web-effects/src/main.rs), [Slint](../../examples/slint-web-effects/src/main.rs), [Iced](../../examples/iced-web-effects/src/main.rs), [GTK4 and libadwaita](../../examples/gtk4-web-effects/src/main.rs), [GPUI](../../examples/gpui-web-effects/src/main.rs) and [libcosmic](../../examples/libcosmic-web-effects/src/main.rs). GTK4, libcosmic and GPUI delegate placement to the toolkit widget. They are compile-checked, not visually verified.

Helpers used below (std only, verified):

```rust
/// Pick below or above the anchor, flipping when there is not enough room.
#[derive(Debug, PartialEq)]
pub enum Side { Below, Above }
pub fn side(anchor_top: f32, anchor_bottom: f32, popup_h: f32, window_h: f32, gap: f32) -> Side {
    let below = window_h - anchor_bottom - gap;
    let above = anchor_top - gap;
    if below >= popup_h || below >= above { Side::Below } else { Side::Above }
}
/// Keep the popup inside the window vertically, after choosing a side. Without it, a window
/// too short for either side puts the popup partly off screen.
pub fn clamp_y(y: f32, popup_h: f32, window_h: f32, margin: f32) -> f32 {
    y.clamp(margin, (window_h - popup_h - margin).max(margin))
}
/// Keep the popup inside the window horizontally ("shift").
pub fn clamp_x(anchor_center: f32, popup_w: f32, window_w: f32, margin: f32) -> f32 {
    (anchor_center - popup_w / 2.0).clamp(margin, (window_w - popup_w - margin).max(margin))
}
fn lin(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}
pub fn luminance(rgb: [u8; 3]) -> f32 { 0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2]) }
pub fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}
/// Black or white, whichever reads better on `bg` (web: `contrast-color()`).
pub fn on_color(bg: [u8; 3]) -> [u8; 3] {
    if contrast(bg, [0, 0, 0]) >= contrast(bg, [255, 255, 255]) { [0, 0, 0] } else { [255, 255, 255] }
}
```

### Anchored popover (web: anchor positioning, `position-area`, `position-try-fallbacks`)
Recipe: read the anchor's window rectangle, choose a side with `side(...)` (flip), clamp vertically with `clamp_y(...)` (a window too short for both sides would otherwise push the popup off screen; it then covers its anchor), shift horizontally with `clamp_x(...)`, size to fit the remaining space, place an arrow at the anchor center, and recompute on resize and scroll. Offer the same placement for tooltips, menus and dropdowns through one component.
Hooks: `gtk::Popover` with `set_pointing_to` and `set_position` flips by itself; Slint `PopupWindow` positioned from the anchor's `absolute-position`; egui an `Area` or the toolkit popup helpers at `response.rect`; Iced overlay or `tooltip` position; GPUI `anchored()` with window snapping.
Don't: hard-code offsets from the window corner, or leave a popover cut off at the window edge.

### Layered popover with light dismiss (web: Popover API, top layer, `closedby`)
Recipe: render overlays in a layer above content, close on Escape and on a click outside (light dismiss), return focus to the trigger, make modal ones block the content behind them, and let only one non-modal popover of a kind stay open. Keep an explicit close control for dialogs that matter.
Hooks: GTK popovers dismiss automatically; Slint `PopupWindow` with an outside-click close policy; egui `Area` plus `Order::Foreground` and a check for a click elsewhere; GPUI `deferred()` for layering and a mouse-down-outside handler.
Don't: use light dismiss for dialogs that hold unsaved input.

### Declarative commands (web: invoker commands `commandfor`, `command`)
Recipe: define actions with ids (open dialog, close dialog, toggle panel) and let buttons, menus and shortcuts all invoke the same action by id instead of each owning a callback. The action owns state and focus handling, so keyboard, tooltips and accessibility stay in one place.
Hooks: `gio::SimpleAction` and `app.` or `win.` actions in GTK; GPUI actions; a command enum in the Iced `Message` and egui model; Slint callbacks forwarded to one handler.
Don't: add an action layer for a single button.

### Rich select (web: customizable `<select>`, `appearance: base-select`)
Recipe: a trigger showing the selected item (icon, two lines allowed), a popover list with the same rendering per option, type-ahead, arrow keys, Enter and Escape, the selected option marked beyond color, and an accessible name and role for the control.
Hooks: GTK `DropDown` with a list item factory, `adw::ComboRow`; Slint a `PopupWindow` with a `ListView`; egui `ComboBox` with custom item contents; Iced a custom overlay or `combo_box`; libcosmic its dropdown widget.
Don't: replace a plain select that works, or lose keyboard and screen-reader behavior.

### Hover and focus preview (web: `interestfor`, `popover="hint"`)
Recipe: open after a short hover delay (about 300 to 500 ms) or on keyboard focus, close with a short grace period when the pointer moves into the preview, close on Escape. The preview repeats information reachable another way.
Hooks: tooltip APIs: GTK query-tooltip, egui hover UI, Slint a small popup.
Don't: put essential information only in a preview, or use it on touch-only surfaces.

### Auto-growing text input (web: `field-sizing: content`)
Recipe: measure the laid-out text height, set the control's height to it between a minimum and a maximum number of lines, and scroll after the maximum.
Hooks: GTK `TextView` in a scrolled window with natural height propagation; Slint preferred height of the text area; egui multiline `TextEdit` with desired rows; Iced and GPUI measure then set the height.
Don't: grow without a maximum, or shift the surrounding layout while typing.

### Balanced headings (web: `text-wrap: balance`, `pretty`)
Recipe: lay the heading out at the container width and count lines N; binary search the smallest width that still yields N lines and use it as the maximum width; limit to headings of up to about four lines. For `pretty`, avoid a last line of one short word by narrowing the width slightly when it occurs.
Don't: apply to long paragraphs, or recompute on every frame (cache by text and width).

### Optical text alignment (web: `text-box-trim`, `text-box-edge`)
Recipe: use the font's cap height and baseline from its metrics to offset text so it centers visually inside buttons and badges, instead of centering the line box.
Don't: trim multi-line body text without checking descenders.

### Automatic on-color (web: `contrast-color()`)
Recipe: choose the text color for a dynamic background with `on_color(bg)`, then verify the ratio with `contrast(...)`: at least 4.5:1 for normal text. For brand or themed pairs use designed on-colors instead.
Don't: assume black or white always passes (mid-tone backgrounds can fail both), or use it for non-text contrast.

### Light and dark values (web: `light-dark()`)
Recipe: store each role as a pair (light, dark) and resolve it from the system color scheme, updating live when the setting changes; recheck contrast per pair.
Don't: derive dark from light by inversion.

### Component-level responsiveness (web: container queries, anchored container queries)
Recipe: choose a variant from the component's own available width (compact, regular, wide), not the window's, and from the popover's flipped side for arrows and corners.
Hooks: Iced `responsive`; libadwaita breakpoints and `adw::BreakpointBin`; egui `ui.available_width()`; Slint the component's own width; GPUI measure bounds.
Don't: use window size for a component that can sit in different panes.

### Custom shapes and corners (web: `shape()`, `border-shape`, `corner-shape`)
Recipe: draw the outline as a path (rounded rectangle, bevel, notch or a superellipse with exponent around 4 to 5 for squircle-like corners) and use the same path for clipping, border and shadow so they follow the shape; keep a plain rounded rectangle as the fallback for hit-testing and focus rings.
Hooks: egui path shapes; Slint `Path`; Iced `canvas`; GTK custom snapshot or CSS radius only; GPUI paths.
Don't: use unusual shapes where focus rings, touch targets or border contrast would suffer.

### Exclusive accordion (web: `<details name>`)
Recipe: keep the expanded section in the model as an optional index; opening one collapses the other with the animate-to-natural-size tactic.
Hooks: `adw::ExpanderRow` groups need manual coordination; egui and Iced keep the index in state.
Don't: make exclusive sections that users need to compare side by side.

### Reading order equals focus order (web: `reading-flow`)
Recipe: make keyboard focus order follow the visual and reading order; when a layout reorders items visually, reorder the focus chain with it.
Hooks: GTK focus chain, Slint `FocusScope` order, egui and Iced widget order, GPUI focus handles.
Don't: use positive tab indices or leave focus jumping across the screen.

### Divider between items (web: `column-rule`, `row-rule`, gap decorations)
Recipe: draw separators between children in the container's paint pass, spanning only the gap, not inside item padding.
Don't: draw dividers around every item when spacing alone separates groups.
