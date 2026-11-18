# relm4

Shared rules for verification, existing projects and review: [common](../common.md).

Use for message-driven gtk applications. Crate signals: `relm4`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://relm4.org/book/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Read GTK4 as a companion and libadwaita only if used. Start from a matching official component example.

## State and interaction

Model input/output messages explicitly. Use factories for collections where appropriate and version-supported workers/tasks for background operations. Keep UI updates on the main context.

## Platform and compatibility

Do not transplant Iced update/view or GPUI context APIs. Check Relm4 features against GTK and optional libadwaita system-library requirements.
