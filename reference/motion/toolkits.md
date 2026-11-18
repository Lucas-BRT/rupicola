# Motion in each toolkit

Shared rules: [common](../common.md), [index](index.md). APIs below are the usual entry points. Confirm each against the project's pinned version; anything marked verify is from memory and may have changed. Always check the reduced-motion setting of the target platform.

## Web family (Leptos, Yew, Dioxus web, Topcoat, Tauri frontends)

Support differs by feature and by engine; check status and fallbacks in [modern web features](../web/modern-css.md) before using anything below that is not widely available.

CSS first. Transitions for state changes, `@keyframes` for sequences, `transform` and `opacity` only (compositor friendly). Entry animations on inserted elements: `@starting-style` and `transition-behavior: allow-discrete` for `display` and overlay transitions (verify support). Spring-like easing: the CSS `linear()` function. Page and shared-element transitions: View Transitions API (`document.startViewTransition`, `view-transition-name`). Scroll-linked effects: `animation-timeline: scroll()` or `view()`. Imperative control: Web Animations API (`element.animate`). Reduced motion: `@media (prefers-reduced-motion: reduce)`.

## egui

Immediate mode: motion needs a repaint request. `ctx.animate_bool(id, value)` and `ctx.animate_bool_with_time(id, value, seconds)` return a 0 to 1 factor; `ctx.animate_value_with_time(id, target, seconds)` tweens a float; the `emath::easing` helpers shape the curve (verify names). While something is animating egui keeps repainting; when idle it should stop to save CPU, so avoid unconditional `request_repaint()`. There is no built-in reduced-motion hook, so read the platform setting yourself or expose an app option, and set the animation time to zero when it is on.

## Slint

Declarative: `animate x { duration: 200ms; easing: ease-out; }` on a property, `states [...]` with `in { animate ... }` and `out { ... }` transitions for state changes, and `cubic-bezier(...)` easings. `Timer` for time-driven logic. Prefer animating position, opacity and color; keep animated elements out of layouts that recompute every frame. Reduced motion: expose a property bound to the platform preference or an app setting and set durations to 0.

## GTK4 and libadwaita (gtk4-rs, Relm4, libadwaita)

Widgets: `gtk::Revealer` (slide, crossfade reveal), `gtk::Stack` transition types, `adw::NavigationView` and `adw::ViewStack` with built-in transitions. Custom: `adw::TimedAnimation` (duration and easing) and `adw::SpringAnimation` (spring parameters) with an `adw::CallbackAnimationTarget` updating a property (verify constructors for the version). The user setting `gtk-enable-animations` disables animation globally; libadwaita animations respect it, so prefer them over custom timers. Avoid animating via repeated `queue_draw` timers.

## Iced

`iced::animation::Animation` (interpolated values with easing; verify it exists in the pinned version). Per-frame driving: `iced::window::frames()` subscription that ticks while an animation is active and is dropped when idle. Keep animation state in your model, advance it in `update`, read it in `view`. No built-in reduced-motion switch; pass an app flag.

## libcosmic

Built on Iced; look at the libcosmic animation helpers and the `cosmic-time` crate for timeline-style animation (verify the crate and API for the version). Respect the COSMIC theme's motion preference where it exposes one; otherwise an app setting.

## GPUI

Elements can be animated with `with_animation(id, Animation::new(duration).with_easing(...), |element, delta| ...)` (verify the trait and signature for the revision). The closure maps a 0 to 1 progress to element styles; keep animated properties to transforms, opacity and colors. Request repaints only while active.

## Native desktop in general

For reproducing newer web effects (shared element, scroll-linked animation, carousels, staggered entry) see [native effects](native-effects.md).


Respect the platform: Windows client-area animation setting, macOS Reduce Motion, GNOME `gtk-enable-animations`, COSMIC and KDE settings. Prefer toolkit animation primitives over timers, and verify frame pacing on the slowest supported hardware.
