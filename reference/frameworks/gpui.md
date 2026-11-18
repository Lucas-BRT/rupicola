# gpui

Shared rules for verification, existing projects and review: [common](../common.md).

Use for custom-rendered desktop tools. Crate signals: `gpui, gpui_platform`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://github.com/zed-industries/zed/tree/main/crates/gpui). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Start from a matching official example. Application opens a window whose root Entity implements Render. Keep state in entities and express commands through actions.

## State and interaction

Use the framework executor and contexts; do not hold mutable context borrows across asynchronous work. Preserve focus handles, stable element IDs and action dispatch. Virtualize large lists.

## Platform and compatibility

div() is not DOM. Do not prescribe CSS or ARIA. Check platform backend features, text input, IME and the actual accessibility implementation.

## Version gotchas

Checked by compiling [the example](../../examples/gpui-web-effects/src/main.rs) against gpui 0.2.2; recheck for any other revision. With a fresh dependency resolution gpui 0.2.2 does not build on Linux: it pulls `xattr 0.2.3` (through `zed-async-tar`), which uses `libc::ENOATTR`, removed in libc 0.2.190 while `mio` requires libc 0.2.190 or newer. The example carries a one-line patched copy in `vendor/xattr` through `[patch.crates-io]`. System libraries (xcb, xkbcommon, fontconfig, wayland) are needed to link. Entry point: `Application::new().run(|cx| cx.open_window(..))` and `Render::render(&mut self, window, cx)`. `anchored().snap_to_window_with_margin(..)` inside `deferred(..).with_priority(..)` places overlays and snaps them into the window; `on_mouse_down_out` is the light dismiss. `with_animation(id, Animation::new(duration).with_easing(gpui::ease_in_out), |el, delta| ..)` restarts when the id changes. `canvas(prepaint, paint)` can record an element's `Bounds` during prepaint; `ScrollHandle::offset()` gives the scroll position for scroll-linked effects.

Found by running the example on a virtual X display (software Vulkan) and looking at the screenshots: a ghost element kept in state after its close animation stayed on top of the card and swallowed its clicks, so it was dropped with a delayed task (`cx.spawn(async move |this, cx| { cx.background_executor().timer(..).await; this.update(cx, ..) })`). A scroll-linked reveal left the last rows dim at the end of the list; compare against the real scroll range from `ScrollHandle::max_offset()` rather than constants (a hand-computed range was off by a row gap and never triggered). Card labels are top left unless the card is `flex().items_center().justify_center()`. The first frame was black under Xvfb until the first input event. Windows opened centered, so screen coordinates differ from window coordinates. Run it with `env -u WAYLAND_DISPLAY DISPLAY=:99 VK_ICD_FILENAMES=<lavapipe icd>`.
