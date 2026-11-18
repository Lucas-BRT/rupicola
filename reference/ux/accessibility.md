# Operating with diverse abilities

Shared adaptation rules: [common](../common.md). The rules below are self-contained; do not fetch the source to apply them.

## Rules

1. Every interactive element has a role, an accessible name and its state exposed (WCAG 4.1.2). A placeholder is not a label (3.3.2); icon-only buttons need a name.
2. Errors are identified in text and tied to the field that caused them (3.3.1). Status changes such as "saved" or "3 results" are announced without moving focus (4.1.3).
3. Text scales to 200% without loss of content or function (1.4.4); layouts reflow at narrow widths instead of clipping (1.4.10).
4. Do not rely on color, position or sound alone to convey meaning. See the color, input and motion modules for numeric thresholds.
5. Native toolkits expose a widget tree to the platform (AT-SPI on Linux, UI Automation on Windows, NSAccessibility on macOS). Custom-drawn UIs expose nothing unless the toolkit's accessibility backend is on; some toolkits use AccessKit. Verify support per toolkit and version instead of assuming it.

## Examples

- Leptos: `<label for="email">"Email"</label><input id="email" type="email" aria-invalid=move || (!valid.get()).to_string() aria-describedby="email-error"/><p id="email-error" role="alert">...</p>` ties label, state and error text together.
- Slint: `accessible-role: button; accessible-label: "Undo";` on a clickable `Rectangle`; without them assistive tech sees nothing.
- Bad: "Saved" appears silently. Good: render it in a `role="status"` region (web) or the toolkit's announcement API so it is spoken without moving focus.
- Bad: a delete control made from a `GestureClick` on a plain box (not focusable, no role). Good: a real button widget, which brings focus, keyboard activation and a role.

## Check

Inspect the exposed names, roles and states with the platform inspector or a screen reader, and operate the changed flow by keyboard. An accessibility library in Cargo.toml or a clean screenshot does not prove the tree is correct. Report each check you could not run as unverified.

## Adapt

Apply DOM techniques only to DOM renderers; desktop widgets use their own semantic mechanisms.

[Source](https://www.w3.org/WAI/WCAG22/), kept for audit; WCAG 2.2 AA is the reference level.
