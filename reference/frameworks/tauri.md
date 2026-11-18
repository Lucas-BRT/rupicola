# tauri

Shared rules for verification, existing projects and review: [common](../common.md).

Use for desktop shell with a web frontend. Crate signals: `tauri`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://v2.tauri.app/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Compose this module with the chosen Rust frontend. Inspect tauri.conf.json, capabilities and existing commands before adding integration.

## State and interaction

Validate IPC inputs and authorize operations in Rust. Keep filesystem/shell permissions narrowly scoped. Separate local UI navigation from external links and remote content.

## Platform and compatibility

Windows WebView2 and macOS/Linux WebKit differ. Verify the supported engine policy. Packaging, signing and updater behavior depend on OS and distribution.
