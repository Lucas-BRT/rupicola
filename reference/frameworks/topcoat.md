# topcoat

Shared rules for verification, existing projects and review: [common](../common.md).

Use for server-rendered web applications. Crate signals: `topcoat`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://github.com/tokio-rs/topcoat). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Inspect the matching version of the existing Topcoat guide and upstream examples. Use routed pages and components; markup lives in view! and class composition may use class!.

## State and interaction

Keep validation and authorization on the server. Preserve real links and form semantics. Inspect generated styles and caller/component class conflicts rather than assuming Tailwind resolves them.

## Platform and compatibility

No upstream manual is bundled. Read the official documentation or source at the project's resolved Topcoat version; do not apply a different release's macro or runtime APIs.
