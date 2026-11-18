# gtk4

Shared rules for verification, existing projects and review: [common](../common.md).

Use for gtk desktop applications. Crate signals: `gtk4`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://gtk-rs.org/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Start from the matching gtk-rs application example. Inspect application/window lifecycle and GLib main-context integration.

## State and interaction

Keep widget access on the appropriate main thread. Use weak references where signal captures would retain widgets cyclically. Update list models instead of rebuilding all rows.

## Platform and compatibility

System GTK libraries and versions matter. Linux integration does not prove Windows/macOS packaging or behavior. Follow the destination desktop conventions.

## Version gotchas

Checked by compiling [the example](../../examples/gtk4-web-effects/src/main.rs) against gtk4 0.11 and libadwaita 0.9; recheck for any other version. `gtk::Popover` flips and stays inside the window by itself and `set_autohide(true)` is the light dismiss, so no placement code is needed. `Widget::compute_bounds(&ancestor)` returns the rectangle in that ancestor's coordinates (use a `gtk::Overlay` for ghosts). `adw::TimedAnimation::new(&widget, from, to, ms, adw::CallbackAnimationTarget::new(..))` with `set_easing(adw::Easing::..)`, `connect_done` and `play()`; the user setting is `gtk::Settings::is_gtk_enable_animations()`. Linking needs the GTK4 and libadwaita development files; a `cargo check` still needs their pkg-config entries.

Found by running the example on a virtual X display (`GDK_BACKEND=x11 GSK_RENDERER=cairo`) and looking at the screenshots: a bare `gtk::Frame` used as a ghost draws only a thin border and looks hollow; use a `gtk::Box` with the libadwaita `card` style class (also good for list rows). The popover opened under the button and closed on Escape and on an outside press. Without a compositor the popover surface shows a black rectangle around it; that is the virtual display, not the example.
