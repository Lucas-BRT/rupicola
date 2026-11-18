# Layout and structure tactics

Shared rules: [common](../common.md), [index](index.md). Thresholds: [layout module](../ux/layout.md), [perception](../ux/perception.md).

### Proximity grouping
Use when: related fields, actions or facts share a screen. Do: set space inside a group clearly smaller than between groups (about 1:2 or more) and add a heading only when proximity alone is not enough. Avoid: boxes around everything; borders replace spacing badly.

### Spacing scale
Use when: any layout. Do: use a small set of steps (for example 4, 8, 12, 16, 24, 32) from tokens and one step for related gaps, a larger one for section gaps. Avoid: one-off pixel values and equal gaps everywhere.

### Alignment grid
Use when: forms, cards, toolbars. Do: align to one left edge per column, align text baselines in a row, align numbers right, and use optical alignment for round icons that look off by a pixel. Avoid: centered body text and ragged edges between sibling blocks.

### Content max width
Use when: reading text or forms on wide windows. Do: cap at about 45 to 75 characters for prose and about 480 to 640 px for forms, centered or left-anchored, with margin absorbing extra width. Avoid: full-width inputs on a 4K window.

### Adaptive panes
Use when: sidebar plus content, master-detail. Do: show both at wide sizes, collapse the sidebar into an overlay or the detail into a pushed page below a breakpoint, and keep state on resize. Avoid: shrinking both panes until neither is usable; define a minimum window size.

### Master-detail and split view
Use when: a list where the user inspects or edits one item. Do: keep the list in place, show the detail beside it, preserve selection and scroll, support keyboard arrows. Avoid: opening a dialog per item for read-only detail.

### Progressive disclosure
Use when: advanced options, rare actions, long details. Do: show the common 20% by default, reveal the rest through a labeled expander, "Advanced" section or detail view, and remember the choice. Avoid: hiding something users need often or putting required fields inside a collapsed area.

### Primary action placement
Use when: dialogs, forms, wizards. Do: one visually dominant primary action in the platform's expected place (dialog end on web and most desktop, header bar on GNOME), secondary quieter, destructive separated. Avoid: two equal filled buttons and moving the primary action between screens.

### Sticky and persistent regions
Use when: long scrolling content with controls that must stay reachable (action bars, table headers, form submit). Do: keep them sticky with a subtle edge when content scrolls under, and make sure they never cover the focused field (WCAG 2.4.11). Avoid: stacking several sticky bars that eat the viewport.

### One scroll region
Use when: complex screens. Do: let one region scroll per axis, keep headers and toolbars fixed, and use `overflow` containers on purpose. Avoid: nested same-direction scrollers and horizontal scrolling of whole pages.

### Density modes
Use when: data-heavy tools used for hours. Do: offer comfortable and compact densities (row height, padding) through tokens, default to the platform's. Avoid: compact rows with targets under the minimum size for touch.

### Empty space as structure
Use when: the screen feels crowded. Do: remove an element before shrinking spacing; use whitespace to separate groups and give the primary action room. Avoid: filling space because it exists.

### Safe areas and window chrome
Use when: custom title bars, fullscreen, notches, mobile-like surfaces. Do: keep content out of window controls, drag regions and system insets; keep a visible title or drag area. Avoid: controls under client-side decorations.

### Layout stability
Use when: content loads or changes size. Do: reserve space for images, badges and async content so nothing jumps; animate a size change if it must happen. Avoid: layouts that shift under the pointer.

### Overflow handling
Use when: long names, many tabs, many toolbar items. Do: wrap, truncate with the full text available, or move extra items into a "more" menu; test with the longest realistic input. Avoid: clipped text with no way to read it.
