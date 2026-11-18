# dioxus

Shared rules for verification, existing projects and review: [common](../common.md).

Use for rust component ui across renderers. Crate signals: `dioxus, dioxus-desktop, dioxus-web`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://dioxuslabs.com/learn/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Identify the actual renderer and features first. Use a matching official dx template/example and component/rsx syntax.

## State and interaction

Keep signals, effects and asynchronous tasks within documented lifecycle rules. Dispose resources on unmount and prevent stale results from replacing newer input.

## Platform and compatibility

Desktop WebView and alternative renderers have different APIs and semantics. Only apply DOM/CSS rules when the actual renderer supports them.
