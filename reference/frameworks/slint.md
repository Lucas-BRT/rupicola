# slint

Shared rules for verification, existing projects and review: [common](../common.md).

Use for declarative desktop ui with rust models. Crate signals: `slint`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://docs.slint.dev/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Use a matching official Rust example and distinguish .slint declarations from Rust callbacks/model integration.

## State and interaction

Keep declarative properties and model updates synchronized. Move heavy work off the UI thread and return updates using the version-supported event-loop API.

## Platform and compatibility

Check backend availability, accessibility, build integration and applicable licensing terms before distribution. A web backend does not imply equivalent accessibility.

## Version gotchas

Checked by compiling [the example](../../examples/slint-web-effects/src/main.rs) against slint 1.19.0; recheck for any other version. Widgets such as `CheckBox` need `import { CheckBox } from "std-widgets.slint";`. `Flickable` `viewport-y` and `viewport-height` are deprecated: use `content-y` and `content-height`. Top-level functions are not allowed: put reusable functions in a `global` (`export global Motion { pure public function .. }`) and call `Motion.name(..)`. Add `#[edition = "1.19"]` to the `slint!` macro to avoid the language-edition warning. Inside the `slint!` macro, hex colors such as `#466eb4` break the Rust lexer (read as an exponent) and `\{}` string interpolation does not parse: use `rgb(..)` and string concatenation, or move the UI to a `.slint` file built with `slint-build`. On Linux the default build needs the fontconfig development files.

Found by running the example on a virtual X display and looking at the screenshots: in a `VerticalLayout` without `alignment: start`, spare window height is spread over the headings and leaves large gaps. Under a bare X server (no window manager) `preferred-width`/`preferred-height` on the `Window` were ignored (the window took the layout's width, 378 px, instead of 560): set `width` and `height` for the initial size. `PopupWindow` with `close-policy: close-on-click-outside` closed on an outside click as intended. Run it with `env -u WAYLAND_DISPLAY WINIT_UNIX_BACKEND=x11 DISPLAY=:99` so it uses the virtual display and not the desktop session.
