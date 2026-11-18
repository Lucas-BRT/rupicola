# Native equivalents of modern web motion effects

Shared rules: [common](../common.md), [index](index.md). The web gained declarative effects (view transitions, scroll-driven and scroll-triggered animations, scroll-state queries, native carousels, `sibling-index()`, animating to `auto`). A native toolkit gives more control, so you reproduce the behavior from state you already own: widget rectangles, scroll offsets and a clock. Each card names the web effect, the toolkit-neutral recipe, the usual toolkit hooks (confirm against the pinned version) and when not to use it. Use these only when the situation in the [index](index.md) calls for them and `motion` allows; every one needs a reduced-motion fallback (cut or fade).

Compiled examples of the shared element, scroll-linked reveal and popover: [egui](../../examples/egui-web-effects/src/main.rs), [Slint](../../examples/slint-web-effects/src/main.rs), [Iced](../../examples/iced-web-effects/src/main.rs), [GTK4 and libadwaita](../../examples/gtk4-web-effects/src/main.rs), [GPUI](../../examples/gpui-web-effects/src/main.rs) and [libcosmic](../../examples/libcosmic-web-effects/src/main.rs) (without the shared element). They are compile-checked, not visually verified.

Helper used by several recipes (std only, verified):

```rust
/// 0..=1 position of `offset` between `start` and `end`.
pub fn progress(offset: f32, start: f32, end: f32) -> f32 {
    if (end - start).abs() < f32::EPSILON {
        return if offset >= end { 1.0 } else { 0.0 };
    }
    ((offset - start) / (end - start)).clamp(0.0, 1.0)
}
pub fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
pub fn stagger_delay_ms(index: usize, step_ms: u32, cap: usize) -> u32 { index.min(cap) as u32 * step_ms }
```

### Shared element transition (web: `view-transition-name`)
Recipe: before the change, record the source widget's rectangle in window coordinates under a stable id; after layout, record the target's. Show a ghost (an image or a drawn copy) at the source rect in an overlay layer, tween its position, size and corner radius to the target rect, crossfade the surrounding content, then remove the ghost and reveal the real target. Keep the ghost non-interactive and cancel it if the user acts.
Hooks: GTK `Widget::compute_bounds` with `gtk::Overlay` for the ghost; Slint `absolute-position` of two elements and a ghost `Rectangle` animated with `animate`; egui store `response.rect` per id in memory, draw the ghost on a foreground layer and drive it with `ctx.animate_value_with_time`; Iced measure bounds through a widget operation and draw the ghost in an overlay; GPUI measure element `Bounds` and render the ghost with `with_animation`.
Don't: use for unrelated views, when source and target are not clearly the same object, or on frequent navigation.

### Crossfade or directional slide between views (web: cross-document and same-document view transitions)
Recipe: keep both views alive for the duration, fade or slide one out and the other in, then drop the old one; the container size must not jump. Direction follows hierarchy.
Hooks: `gtk::Stack` transition types and `adw::NavigationView` (built in); Slint `states` with `in` and `out` transitions; egui draw both with an opacity from `animate_bool`; Iced and GPUI keep both children and interpolate opacity or offset.
Don't: animate between peer tabs that have no order, or hold input until it ends.

### Element-scoped transition (web: element-scoped view transitions)
Recipe: animate only the changing subtree while the rest of the window stays live and interactive. Retained toolkits animate that container's children; immediate-mode toolkits animate just that region's draw call.
Don't: freeze the whole window for a local change.

### Scroll-linked animation (web: scroll-driven animations, `animation-timeline`)
Recipe: read the scroll offset each frame, compute `t = progress(offset, start, end)`, and set opacity, translate or scale as `lerp(a, b, t)`. No clock and no easing is involved: the offset is the timeline, so it is stateless and reversible. For a per-item reveal use the item's rectangle against the viewport as the start and end.
Hooks: GTK `gtk::Adjustment` `value-changed`; Slint `Flickable.viewport-y`; egui the offset in the `ScrollArea` output state; Iced `scrollable` `on_scroll` viewport offset; GPUI the scroll handle offset.
Don't: hide essential content until scrolled, tie long reading text to it, or run many linked effects on one screen. At the end of a list the last rows never leave the entry band, so they stay dim; when the offset reaches the real maximum (ask the scroller for its range, do not recompute it from row constants) show them fully.

### Scroll-triggered one-shot animation (web: `timeline-trigger`, `animation-trigger`)
Recipe: keep a `seen` flag per item; when its rectangle first intersects the viewport (with a margin), start a normal time-based animation once and set the flag.
Don't: replay on scroll back, or leave content invisible if the trigger never fires.

### Scroll-state styling (web: `scroll-state()` stuck, snapped, scrollable)
Recipe: derive booleans from offset and sizes: stuck when `offset > header_top`, at-end when `offset + viewport >= content - 1`, scrollable when `content > viewport`. Use them to show a header shadow, edge fades or a back-to-top button.
Don't: let function depend on the visual state alone.

### Scroll-spy (web: `scroll-target-group`, `:target-current`)
Recipe: the active section is the last whose top is at or above `offset + margin`; highlight its entry and expose it as the selected item for assistive technology; clicking an entry scrolls to its section.
Don't: let scroll-spy fight a click that just chose a section; suppress updates for the duration of the programmatic scroll.

### Carousel with markers and buttons (web: `::scroll-marker`, `::scroll-button()`)
Recipe: a snapping scroller, a marker per page whose selected state follows the page index derived from the offset, and previous and next buttons that scroll by about 85% of the viewport. Keep keyboard arrows and focus visible.
Hooks: `adw::Carousel` with `adw::CarouselIndicatorDots`; others combine the scroller offset with a row of indicators.
Don't: hide content that only the controls can reach.

### Stagger by position (web: `sibling-index()`, `sibling-count()`)
Recipe: the list builder knows each item's index; delay is `stagger_delay_ms(index, 30, 8)` so the first items cascade and the rest appear together. In immediate mode, start each item's `animate_bool` at its own start time.
Don't: re-run it on every refresh or on long lists.

### Animate to natural size (web: `interpolate-size`, `calc-size()`)
Recipe: measure the content's natural height with a layout pass, then tween between zero (or the collapsed size) and that measured value; fade the content slightly later.
Hooks: `gtk::Revealer` does this; Slint preferred-size properties; egui `CollapsingHeader` animates openness; Iced and GPUI need a measure then a tween.
Don't: animate sibling positions per frame in long pages.

### Entry from an initial state (web: `@starting-style`)
Recipe: create the new widget with its animation at progress 0 so the first frame is the starting state, then advance to 1. In immediate mode do not assume a new id starts at 0: egui starts a never-seen `animate_bool` id at its target, so `animate_bool(id, true)` on first sight shows the finished state with no animation. Call it every frame, including while the target is still `false`, and flip the target when the surface opens.
Don't: leave an exit that blocks input.

### Named motion tokens (web: `view-transition-class`)
Recipe: define motion roles once (enter, exit, move, press, emphasis) with a duration, an easing or spring and a distance, and make every component use the role. Changing a role updates every transition.
Don't: scatter literal durations.
