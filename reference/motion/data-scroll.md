# Data, charts and scroll

Shared rules: [common](../common.md), [index](index.md).

### Number tween
Communicates: a value changed by this much. Use when: counters, totals, balances that change under the user's eyes. Recipe: interpolate the displayed number for a short time with tabular figures so the width stays stable; announce the final value once, not each frame. Avoid: counting up on first render for numbers the user reads once.

### Chart update tween
Communicates: object constancy, the same bar became this bar. Use when: filters, time-range changes, live series. Recipe: interpolate each mark from its old to new value and position by identity, not index; enter new marks from the baseline. Avoid: replaying the intro on every update, and animating dense charts frame by frame.

### Chart entrance
Communicates: how the data builds. Use when: first appearance of a chart. Recipe: grow bars from the baseline, draw lines along their path; keep it short and skip it on revisit. Avoid: delaying the data the user came for.

### Scroll-linked reveal
Communicates: content arrives as you reach it. Use when: long editorial pages, onboarding. Recipe: fade and translate a few pixels as the element enters the viewport, once. Web can use scroll or view timelines (`animation-timeline`) without JavaScript; check support and provide a static fallback. Avoid: hiding content until a script runs, and re-animating on scroll back.

### Sticky shrink
Communicates: the header yields space. Use when: long scrolling pages with a large header. Recipe: interpolate header height, title size and shadow with scroll offset; keep controls reachable. Avoid: changing the layout under the pointer while the user is clicking.

### Parallax
Communicates: depth. Use when: marketing and storytelling surfaces only. Recipe: move background layers at a fraction of the scroll speed. Strong vestibular trigger: disable under reduced motion. Avoid: in tools, forms and data views.

### Scroll progress and position indicators
Communicates: how far through. Use when: documents, long forms, timelines. Recipe: a thin bar bound to scroll offset; scrubbers and scrollbars that grow on hover. Avoid: tying essential information to a scroll-only effect.

### Live data stream
Communicates: continuous change without chaos. Use when: logs, metrics, feeds. Recipe: batch updates per frame, animate only new rows entering, pause motion when the user scrolls or hovers, and offer a "jump to latest" control. Avoid: shifting content under a reading user.
