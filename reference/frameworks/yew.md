# yew

Shared rules for verification, existing projects and review: [common](../common.md).

Use for component-based rust web applications. Crate signals: `yew`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://yew.rs/docs/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Identify the selected CSR or SSR/hydration features. Start with a version-matching official example; use components, properties, callbacks and hooks consistently.

## State and interaction

Use effects for browser-only work and clean up subscriptions/listeners. Give repeated children stable keys. Keep form state and asynchronous errors explicit.

## Platform and compatibility

SSR cannot access browser APIs. Hydration requires matching initial structure. Do not assume CSR-only setup provides public-page metadata or server rendering.
