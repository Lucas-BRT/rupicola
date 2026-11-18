# Feedback, microinteractions and loading

Shared rules: [common](../common.md), [index](index.md). Feedback should be immediate and small; loading should reassure without flicker.

### Press
Communicates: the control registered the input. Use when: buttons, cards, list rows. Recipe: a slight scale down (about 0.97) or a darken on pointer down, released on pointer up; apply instantly. Avoid: changing layout.

### Hover lift
Communicates: this is interactive. Use when: cards and rows in pointer UIs. Recipe: a few pixels of translate or an elevation increase over a short time. Avoid: hover-only information; touch has no hover.

### Toggle thumb
Communicates: state flipped. Use when: switches, segmented controls. Recipe: slide the thumb and animate the track color together; keep the label state text. Avoid: ignoring the state when motion is reduced; the final position must still be clear.

### Check draw and success
Communicates: completion. Use when: saved, sent, copied. Recipe: draw a check or morph the button label into a check, hold briefly, then settle; announce the status for assistive tech. Avoid: celebrations for routine actions.

### Error shake and nudge
Communicates: input rejected. Use when: failed login, invalid field. Recipe: a small horizontal nudge of a few pixels, once, plus text; skip it under reduced motion. Avoid: repeating, large shakes, or using it without an error message.

### Label float and inline reveal
Communicates: the field has content or help appeared. Use when: text inputs, validation messages. Recipe: move and scale the label up on focus or content; reveal the message by expanding its slot so nothing jumps. Avoid: hiding the label when the field is filled.

### Highlight on change
Communicates: this value just updated. Use when: live data, a row edited elsewhere, a synced setting. Recipe: tint the background with an accent and fade it to transparent over about a second. Avoid: highlighting continuous streams; throttle it.

### Skeleton
Communicates: layout is coming. Use when: content loads and its shape is known. Recipe: gray placeholders with the final layout, an optional soft shimmer with a period of about 1.2 to 1.8 s, then crossfade to real content with no layout shift. Avoid: skeletons for waits shorter than about a second, and shimmer under reduced motion (use static blocks).

### Spinner with delay
Communicates: working. Use when: unknown duration. Recipe: show the spinner only after about 200 to 300 ms and keep it visible for at least about half a second once shown, to avoid flicker. Provide a cancel for long waits. Avoid: spinners that cover the whole screen for a local operation.

### Determinate progress
Communicates: how long. Use when: uploads, imports, installs. Recipe: a bar or ring that tracks real progress, animate between updates smoothly but never run ahead of the truth; show an estimate and the step name. Avoid: fake progress.

### Optimistic update
Communicates: instant response. Use when: toggles, reorders, likes where failure is rare. Recipe: apply the change immediately, mark it pending subtly, roll back with a message if the request fails. Avoid: for irreversible or financial actions.

### Button loading morph
Communicates: this click started something. Use when: submit buttons. Recipe: disable the button, keep its width, swap the label for a spinner or progress text, restore on completion. Avoid: letting the button change size and shift the layout.
