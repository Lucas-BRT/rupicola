# iced

Shared rules for verification, existing projects and review: [common](../common.md).

Use for message-driven desktop applications. Crate signals: `iced`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://github.com/iced-rs/iced). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Use a version-matching official example and its application entrypoint. Model explicit state, Message, update and view.

## State and interaction

Use Task and subscriptions where the selected version provides them; retain cancellation/identity for asynchronous results. Keep view computation free of blocking IO.

## Platform and compatibility

Do not copy libcosmic APIs into upstream Iced. Verify renderer/backend features, system dependencies and accessibility on each target.

## Version gotchas

Checked by compiling [the example](../../examples/iced-web-effects/src/main.rs) against iced 0.14.0; recheck for any other version. The entry point is `iced::application(App::new, App::update, App::view)` with the boot function first. `iced::animation::Animation` (`new`, `quick`, `go_mut`, `is_animating`, `value`, `interpolate`) is driven by a `window::frames()` subscription that should exist only while animating. Measuring a widget needs `features = ["selector"]` and `iced::widget::selector::find(selector::id(..))`, whose target gives `visible_bounds()`; there is no `container::visible_bounds`. `Space::new()` takes no arguments, `checkbox(bool).label(..).on_toggle(..)`, `stack` layers widgets and `mouse_area` catches clicks outside a popover. Other useful subscriptions: `window::resize_events()` and `keyboard::listen()`.

Found by rendering the example headlessly and looking at the images: a button's label sits at the top left by default, so wrap it (`button(container(text(..)).center(Length::Fill))`) to center it. A popover laid out by padding inside a `stack` lands exactly where `side`, `clamp_x` and the measured anchor put it (checked by measuring pixels: 8 px margin, 180 by 112, directly under the anchor).

Headless capture works in two ways:
- **Tier 1 offscreen RAM rendering (recommended for animations and fast background loops):** render view trees directly into a RAM `tiny_skia::Pixmap` via `iced_tiny_skia` at deterministic timestamps (`0 ms`, `16.67 ms`, `33.33 ms`, ...) with zero window or display server. Feed the premultiplied RGBA buffer to `ferris_atelier::capture::RamCapture` (`PixelFormat::PremultipliedRgba8`) or pipe it to `ferris-atelier video --raw --width W --height H --fps 60 --pixel-format premultiplied-rgba --save-frames <dir> --out <mp4>` for lossless PNGs (`frames`/`sheet`/`diff`), a CFR MP4, and measured per-frame CPU raster time. Add `[profile.dev.package."*"] opt-level = 3` in `Cargo.toml` when profiling CPU-rasterized dev builds: unoptimized `tiny-skia` dev builds were measured around ~14 fps (~70 ms/frame) versus 60 fps with `opt-level = 3`.
- **Snapshot tests with `iced_test`:** `Simulator::with_size(Settings::default(), size, app.view())`, then `snapshot(&Theme::Dark)` and `matches_image(path)` (which writes the PNG when it does not exist) on software Vulkan: run `env -u DISPLAY -u WAYLAND_DISPLAY VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json cargo test` (the lavapipe path varies by distribution). Snapshots are rendered at 2x. The simulator does not run tasks or subscriptions, so set the state a task would have produced (for example the measured rectangles) directly; measurement tasks, Escape handling and animation ticks are therefore not exercised by it. `Simulator::find(selector::id(..))` returns a target with `visible_bounds()`.
