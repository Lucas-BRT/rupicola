# dioxus

Use for rust component ui across renderers. Crate signals: `dioxus, dioxus-desktop, dioxus-web`. Dependency presence is a discovery hint; confirm actual use, features and renderer.

## Version and sources

[Official documentation](https://dioxuslabs.com/learn/). Inspect Cargo.toml, Cargo.lock and installed source before supplying APIs. No newly authored example in this module has been compiled in this environment. Prefer an upstream example at the project's exact tag/revision; never claim latest-version compatibility from this guide alone.

## Build or modify

Identify the actual renderer and features first. Use a matching official dx template/example and component/rsx syntax.

For an existing application, preserve its entrypoint and shared components. For a new application, select the smallest matching official example, verify its license and dependency revision, and build it before extending it. Implement one real task with loading, empty and error states when relevant.

## State and interaction

Keep signals, effects and asynchronous tasks within documented lifecycle rules. Dispose resources on unmount and prevent stale results from replacing newer input.

Keep keyboard navigation, visible focus, labeled controls and meaningful disabled/pressed states. Consult the relevant UX module when the task changes those behaviors.

## Platform and compatibility

Desktop WebView and alternative renderers have different APIs and semantics. Only apply DOM/CSS rules when the actual renderer supports them.

## Verification

Inspect existing project commands first. Run `cargo check --locked -p <package>` for the actual feature/target selection when available, then the framework's documented run/build command. Web/WASM and generated-source pipelines may require their own CLI rather than a host-only Cargo check. Exercise the changed task, failure/retry, keyboard focus, resize and text scaling. Record the command, features, target and observed result; unavailable execution remains unverified. Distribution follows the relevant OS guide.

## Review counterexample

Do not replace a functioning project architecture merely to match this module. Identify a concrete task failure or requirement before proposing migration or new dependencies.
