# Choreography: several things moving together

Shared rules: [common](../common.md), [index](index.md). Rule of attention: one focal motion at a time; everything else supports it or stays still.

### Stagger
Communicates: order and rhythm. Use when: first load of a list, grid or menu items. Recipe: offset each child by about 20 to 50 ms and cap the total (animate roughly the first 6 to 10 items; the rest appear together) so the last item does not arrive late. Avoid: re-running it on every refresh or filter.

### Sequencing
Communicates: cause and effect. Use when: a confirmation then a result, a collapse then an insert. Recipe: chain steps so each starts as the previous settles, or overlap them by about 30% to feel connected. Avoid: sequences that make the user wait before they can act.

### Hierarchy
Communicates: what is parent and what is detail. Use when: a surface opens with content. Recipe: container first, then primary content, then secondary; or animate the container and fade children slightly later. Avoid: moving every layer with equal weight.

### FLIP reorder
Communicates: this item moved there. Use when: sorting, filtering, drag reorder, inserting into the middle of a list. Recipe: First (record old rect), Last (apply the final layout), Invert (translate each item back to the old rect), Play (animate the translate to zero). Works because only `transform` animates. Native toolkits: record rects before and after the layout pass and tween an offset. Avoid: on very long lists; animate only the visible rows.

### Insert and remove in a list
Communicates: a row was added or taken away. Use when: todo items, notifications, search results. Recipe: insert by expanding the slot and fading in; remove by fading then collapsing the slot so neighbors close the gap. Offer undo for removals. Avoid: removing instantly when the user did not trigger it; they lose track.

### Overlapping transition
Communicates: continuity. Use when: leaving and entering content in the same area. Recipe: start the entering element while the leaving one is about 60 to 70% done instead of waiting. Avoid: overlap that makes both unreadable at the midpoint; use crossfade timing carefully.

### Anticipation and follow-through
Communicates: weight and intent. Use when: playful or expressive products, drag release, toasts. Recipe: a tiny pull the opposite way before a move, a small overshoot that settles after; implement with a spring rather than hand-tuned keyframes. Avoid: in dense professional tools, and for repeated actions.

### Coordinated layout shift
Communicates: reflow was intentional. Use when: a panel opens and neighbors resize. Recipe: animate neighbors with the same duration and easing as the panel so they move as one body. Avoid: letting the main content jump while only the new panel animates.
