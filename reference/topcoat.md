# Topcoat (Rust)

For sites built with [Topcoat](https://github.com/tokio-rs/topcoat): server-rendered HTML from Rust, `view!` templates, Tailwind. Pinned to the version in `topcoat.lock`; the source is `reference/vendor/topcoat-llms-<tag>.txt`. Everything in [web.md](web.md) applies. This file says what changes when the markup lives in Rust.

## Reading the code

- Markup is in `view! { ... }` inside `#[page]` and `#[component]` functions. Pages are `src/app/<page>.rs`; sections are components in a folder beside it.
- Text nodes are quoted: `<p>"text " (expr)</p>`. `(expr)` interpolates. A number in `(count)` is a real value; a quoted number is typed copy: check it against the source of facts the project names (often a generated `facts.rs`), never against memory.
- Classes: fixed `class="a b"`; composed `class=(class!("base", optional, "x" if on else "y"))`; constants are `StaticClass`. Tailwind utilities, no conflict resolution: when a component sets a class, a caller cannot override it. A visual bug may be two utilities fighting, not a missing one.
- `attributes! { id="x" }` and `attrs: Attributes` forward attributes; `class!(BASE, attrs.remove("class"))` merges them.
- Components take named props, then children: `card(item: x)`. Variants are enums (`ButtonVariant::Ghost`).
- `src/components/` is copied UI (`topcoat ui add`), `components.toml` lists it. Treat it like vendor code: fix tokens and the site's own `src/ui/` first.
- Icons are `icon(data: ICON, label: "...")`; no `label` means decorative. A control that is only an icon needs an `aria-label` on the element.
- Dark theme is a `.dark` ancestor on Topcoat UI. Shadows are off by default.

## Floor, in Topcoat terms

| Web floor | In a Topcoat site |
|---|---|
| Real controls | `<button>` and `<a href=(href!(handler))>`; the `button(...)` component renders a `<button>` |
| Visible focus | check the copied component CSS and `styles.css`, not the view |
| URLs mean something | links go through `href!(handler)`; a page that is not routed does not exist |
| Reduced motion | no JS runtime by default; look for `animate-`, `transition-` and keyframes in `styles.css` |
| Contrast | tokens live in `styles.css` (`@theme`); change a token, not each page |
| Nothing invented | numbers come from generated modules; a literal number in `view!` is a finding |

## What the detector does with Rust

`scripts/detect.js` blanks everything outside `view!`, `attributes!` and `class!` and scans the rest as web, keeping line numbers. It sees class strings and markup. It does not see:

- `build.rs`-generated code, `include!`d files, or markup built by another macro
- raw strings `r#"..."#`
- the compiled CSS: a class that Tailwind did not generate has no effect and no hit

A hit in a legend swatch or a status that carries a text label is a triage case, not a finding: say so in the report.

## Checking it

- Serve the site: `cargo run -p <site>` (`HOST` and `PORT` override 127.0.0.1:3000) or `topcoat dev`. If the served CSS is stale, rebuild so `build.rs` regenerates it.
- Screenshot the routes at 375, 1280 and 1920. Pages are server-rendered: no hydration shift, but fonts can swap.
- `topcoat fmt` formats macro bodies; run it with `cargo fmt` before reporting a fix done.
- Tests that read the rendered HTML are the contract for copy and numbers. Run them after a visual fix.
