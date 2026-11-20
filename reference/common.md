# Shared rules

Read with any framework or UX module. These rules were identical across modules and live here once.

## Framework modules

Do not replace a functioning project architecture merely to match this module. Identify a concrete task failure or requirement before proposing migration or new dependencies.

Inspect existing project commands first. Run `cargo check --locked -p <package>` for the actual feature/target selection when available, then the framework's documented run/build command. Web/WASM and generated-source pipelines may require their own CLI rather than a host-only Cargo check. Exercise the changed task, failure/retry, keyboard focus, resize and text scaling. Record the command, features, target and observed result; unavailable execution remains unverified. Distribution follows the relevant OS guide.

Keep keyboard navigation, visible focus, labeled controls and meaningful disabled/pressed states. Consult the relevant UX module when the task changes those behaviors.

For an existing application, preserve its entrypoint and shared components. For a new application, select the smallest matching official example, verify its license and dependency revision, and build it before extending it. Implement one real task with loading, empty and error states when relevant.

## UX modules

Problem: the task may become hard to understand or operate when this concern is ignored.

Use the actual framework and renderer APIs. DOM techniques apply to DOM renderers; desktop widgets require their own semantic/input mechanisms. Preserve applicable OS conventions and project identity. These are contextual heuristics unless the linked standard defines an applicable requirement.


## Snippets

Code and markup in UX examples are illustrative. Toolkit APIs change between versions; confirm each call against the project's pinned version before using it.

## Anti-slop & structural defaults

Avoid the recurring failure modes of AI-generated interfaces:

1. **All five UI states:** never ship only the populated happy path. Implement loading (without layout shift), empty (concise, actionable), recoverable error (inline + focused), pending/disabled (prevent duplicate submits), and extreme text lengths (`min-width: 0`, `overflow-wrap: anywhere`).
2. **Realistic data stress:** test with `0`, `1`, and `40+` items, long unbroken strings, and non-uniform counts instead of 3 symmetric placeholder cards.
3. **Keyboard and focus integrity:** use native `<button>`/`<a>`, preserve `:focus-visible`, restore focus to the opener on modal `Escape`, and never leave focus on a `disabled` control.
4. **True responsiveness:** verify both `1440px` and `320px` (tables, tab rails, `<code>`/`<pre>`, flex headers) with zero horizontal overflow.
5. **Semantic color and contrast:** keep `>= 4.5:1` contrast, never replace `<label>` with placeholders (WCAG 3.3.2), and reserve accent/alert borders for active or non-zero states (never highlight `0` counts as urgent).
6. **Intentional visual craft:** use `oklch()` tinted neutrals (`chroma 0.005–0.015`, no pure `#000`/`#fff`), three typographic roles (display, UI sans, `tabular-nums` monospace for data/IDs/dates), `1px` hairline grids or dark-mode `inset 0 1px` top rims instead of blurred floating cards, intentional concentric radii (`R_outer = R_inner + padding`), and calibrated density.
7. **Physics-based motion:** avoid `transition: all` and hover `translateY` jitter; animate `transform`/`opacity` with `cubic-bezier(0.16, 1, 0.3, 1)`, add `:active { transform: translateY(1px); }`, and honor `prefers-reduced-motion`.
