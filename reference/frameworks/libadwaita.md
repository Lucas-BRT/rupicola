# libadwaita

Shared rules for verification, existing projects and review: [common](../common.md).

Use for adaptive gtk applications following gnome conventions. Crate signals: `libadwaita`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Read the GTK4 module as a companion. Use a matching official Adwaita application/window example and existing adaptive navigation components.

## State and interaction

Use semantic styles and system appearance settings. Prefer adaptive components over fixed desktop geometry. Preserve GTK focus, actions and lifecycle guarantees.

## Platform and compatibility

GNOME conventions are contextual, not universal Linux rules. Verify minimum libadwaita version and availability on intended distribution targets.
