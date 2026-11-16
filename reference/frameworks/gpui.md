# gpui

Use for custom-rendered desktop tools. Crate signals: `gpui, gpui_platform`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://github.com/zed-industries/zed/tree/main/crates/gpui). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Start from a matching official example. Application opens a window whose root Entity implements Render. Keep state in entities and express commands through actions.

For an existing application, preserve its entrypoint and shared components. For a new application, select the smallest matching official example, verify its license and dependency revision, and build it before extending it. Implement one real task with loading, empty and error states when relevant.

## State and interaction

Use the framework executor and contexts; do not hold mutable context borrows across asynchronous work. Preserve focus handles, stable element IDs and action dispatch. Virtualize large lists.

Keep keyboard navigation, visible focus, labeled controls and meaningful disabled/pressed states. Consult the relevant UX module when the task changes those behaviors.

## Platform and compatibility

div() is not DOM. Do not prescribe CSS or ARIA. Check platform backend features, text input, IME and the actual accessibility implementation.

## Verification

Inspect existing project commands first. Run `cargo check --locked -p <package>` for the actual feature/target selection when available, then the framework's documented run/build command. Web/WASM and generated-source pipelines may require their own CLI rather than a host-only Cargo check. Exercise the changed task, failure/retry, keyboard focus, resize and text scaling. Record the command, features, target and observed result; unavailable execution remains unverified. Distribution follows the relevant OS guide.

## Review counterexample

Do not replace a functioning project architecture merely to match this module. Identify a concrete task failure or requirement before proposing migration or new dependencies.
