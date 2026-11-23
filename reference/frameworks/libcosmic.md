# libcosmic

Shared rules for verification, existing projects and review: [common](../common.md).

Use for cosmic desktop applications. Crate signals: `cosmic, libcosmic`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://pop-os.github.io/libcosmic-book/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Use the official cosmic-app-template matching the chosen libcosmic revision. Follow its application trait and message/update lifecycle; do not substitute upstream Iced signatures.

## State and interaction

Keep configuration and theme integration in the application layer. Use COSMIC widgets and semantic theme roles. Dispatch background work through the runtime integration provided by that revision.

## Platform and compatibility

COSMIC integrations may depend on Linux services. Verify Wayland/X11, portals, configuration and accessibility separately on each destination OS.

## Version gotchas

Checked by compiling [the example](../../examples/libcosmic-web-effects/src/main.rs) against libcosmic git revision `be7937d` (pinned in the example); recheck for any other revision. Applications implement `cosmic::Application` (`core`, `core_mut`, `init`, `update`, `view`, `subscription`) and run with `cosmic::app::run::<App>(Settings::default(), flags)`. `widget::column` is Iced's helper that takes a children iterable: use `widget::Column::new()` for the builder. `widget::toggler(bool).label(String)`, `Space::new()` takes no arguments. `widget::popover(content).popup(..).on_close(..).position(..)` is the toolkit overlay with light dismiss. `cosmic::anim::{lerp, slerp, State}` is libcosmic's small animation helper (a last-change time plus `t(duration, forward)`), driven by `window::frames()`. Iced's `selector` feature (widget measurement) is not exposed by libcosmic at this revision, so shared-element ghosts that need measured rectangles are not possible without it.

Found by running the example on a virtual X display (software Vulkan) and looking at the screenshots: a detail panel that appears with `Space` before it and the panel after pushes the layout down, so give it a fixed-height slot. `popover(..).on_close(..)` closed on an outside press only; Escape needed a subscription, and `iced::event::listen_with` (which also sees events a widget captured) worked where `keyboard::listen` did not in this run, with the window focused. Startup under software rendering took about 15 seconds. The window needs input focus for keyboard events when there is no window manager.

## Visual capture and animation profiling

Measured on Pop!_OS COSMIC 1.11.0 Wayland: `xvfb-run` fails with `softbuffer` (`Visual 0x40 does not use softbuffer's pixel format` on 24-bit; `Couldn't add screen 0` on 32-bit), Pop!_OS COSMIC ships without headless Wayland compositors (`weston`/`cage`/`sway`/`labwc`), `cosmic-screenshot` takes ~50–100 ms/frame over D-Bus/portal (too slow for 60 fps and intrusive on the live desktop), and panel applets (`cosmic::applet::run`) require `cosmic-panel`'s privileged Wayland socket (`X_PRIVILEGED_WAYLAND_SOCKET`) for `xdg_popup`.

- **Tier 1 (background default for iterative dev & animation checks):** render view trees offscreen into RAM via `iced_tiny_skia` (`tiny_skia::Pixmap`, premultiplied RGBA) at deterministic timestamps to produce lossless PNGs, optional CFR MP4 via `ffmpeg`, and per-frame CPU raster timings with zero display interference. Set `[profile.dev.package."*"] opt-level = 3` in `Cargo.toml` for CPU-rasterized dev builds (prevented ~14.2 fps / 70.3 ms/frame dev bottlenecks vs 60 fps).
- **Tier 2 (final real-compositor check for `xdg_popup`, `autosize`, panel applets, and real 60 Hz `wl_callback`):** use PipeWire `org.freedesktop.portal.ScreenCast` v5 with `persist_mode = 2` (`restore_token` — single one-time approval, zero dialogs on subsequent runs) plus `gstreamer1.0-pipewire` and `ffmpeg` buffered in RAM (`/dev/shm`).
