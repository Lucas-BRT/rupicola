# Motion and feedback timing

Shared adaptation rules: [common](../common.md). The rules below are self-contained; do not fetch the source to apply them.

For the catalog of tactics (transitions, choreography, gesture physics, feedback, loading, data and scroll) and per-toolkit calls, see the [motion library](../motion/index.md).

## Rules

1. Animate only to explain a change (relate a popover to its trigger, show where an item went). Remove motion that only decorates.
2. Honor the system reduced-motion setting and replace movement with an instant change or a fade. Web: `prefers-reduced-motion`. Desktop: read the OS setting (for example GTK `gtk-enable-animations`, Windows client-area animations, macOS Reduce Motion).
3. Feedback for direct input should feel immediate; UI transitions are typically 100 to 300 ms. These are heuristics, not standards: shorten for repeated actions.
4. Never block input on an animation. Repeated keyboard selection must not wait for movement, and transitions must be interruptible.
5. Content that moves, blinks or scrolls for more than 5 seconds needs a way to pause or stop (WCAG 2.2.2). Nothing flashes more than three times per second (2.3.1).
6. On the web animate `transform` and `opacity`; avoid `transition: all`, which animates layout properties and ignores which properties you intended.

## Examples

- Bad: `transition: all 300ms`. Good: `transition: transform 150ms ease-out, opacity 150ms;` and `@media (prefers-reduced-motion: reduce) { .panel { transition: opacity 1ms; } }`.
- Bad: a badge with `animation: blink 0.2s steps(1) infinite` (5 flashes per second). Good: a static badge, or one pulse that stops after a few seconds.
- Bad: a menu ignores clicks for the 400 ms its open animation takes. Good: it accepts input immediately and the animation only decorates.
- Bad: a toast that slides in and vanishes in 2 seconds. Good: it fades in, stays long enough to read (or until dismissed) and is announced as a status message.

## Check

Repeat the interaction rapidly, interrupt it mid-transition and turn reduced motion on; confirm input and focus stay correct and that the reduced version still shows the state change.

[Source](https://www.w3.org/WAI/WCAG22/Understanding/pause-stop-hide.html), kept for audit.
