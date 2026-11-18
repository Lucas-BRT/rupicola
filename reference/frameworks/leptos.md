# leptos

Shared rules for verification, existing projects and review: [common](../common.md).

Use for reactive rust web applications. Crate signals: `leptos`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://book.leptos.dev/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Identify CSR, SSR/hydration or islands before editing. Use a matching cargo-leptos example for full-stack work. Keep browser-only APIs outside server execution.

## State and interaction

Use signals for reactive state and resources/actions for their version-appropriate async roles. Render stable initial server/client structure and preserve request-specific state isolation.

## Platform and compatibility

Hydration mismatches can arise from invalid HTML, nondeterminism or different server/client branches. Verify initial HTML and the hydrated interaction separately.
