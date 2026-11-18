# Motion library

Shared adaptation rules: [common](../common.md). Principles and safety limits are in [motion principles](../ux/motion.md); this library is the catalog of tactics. Load this index first, then at most two category files.

## Proactive protocol

The user does not need to know these tactics exist. While planning, building, improving or reviewing a UI:

1. Scan the work for the situations below.
2. Pick at most three tactics per screen, ordered by value: orientation and continuity first, feedback second, polish last.
3. Act by workflow. Build: implement the tactics the feature's own states require (enter and exit of the new surface, loading, press feedback), minimal. Improve: propose a ranked list with where and why, and apply it when the request is about polish or the user agrees. Review: report missing motion as a defect only when a state change goes unnoticed or disorienting; otherwise as an opportunity. Plan: include a short motion plan.
4. Respect the project: reuse its motion tokens and existing durations, match its personality, and read `motion = off | subtle | expressive` from `.ferris-atelier/context.md` (default `subtle`). Do not expand scope silently; name what was added in plain words ("added a container transform from card to detail").
5. Every tactic ships with its reduced-motion fallback and a way to verify it.

## Situation to tactics

| Situation | Tactics | File |
|---|---|---|
| Element appears or disappears (panel, popover, dialog, toast) | fade, slide, scale from origin, clip reveal | [transitions](transitions.md) |
| Switching between views or tabs; push and pop navigation | directional slide, crossfade, shared element, container transform | [transitions](transitions.md) |
| List changes (insert, remove, reorder, filter) | FLIP reorder, collapse height, stagger on first load | [choreography](choreography.md) |
| Several things change at once | sequencing, hierarchy, stagger, one focal motion | [choreography](choreography.md) |
| Dragging, swiping, flinging, resizing a pane | spring, inertia, rubber band, snap, velocity handoff | [gesture-physics](gesture-physics.md) |
| Pressing, toggling, hovering, validating | press, hover lift, toggle thumb, check draw, label float | [feedback-loading](feedback-loading.md) |
| Waiting on data | skeleton, determinate progress, delayed spinner, optimistic update | [feedback-loading](feedback-loading.md) |
| Value or chart updates; scrolling | highlight on change, number tween, chart tween, scroll-linked reveal, sticky shrink, parallax | [data-scroll](data-scroll.md) |
| Need the actual call for a toolkit | per-toolkit mapping | [toolkits](toolkits.md) |
| Reproducing what the web now does declaratively (view transitions, scroll-driven, carousels, animate to auto) in a native toolkit | scroll-linked mapping, shared element ghost, scroll-state, stagger by position | [native-effects](native-effects.md) |

## Purpose to tactics

Orient (where did it come from, where did it go): slide with direction, shared element, FLIP. Confirm (it worked): press, check draw, highlight on change. Wait (it is working): skeleton, progress, delayed spinner. Attend (look here): single pulse, highlight, badge pop. Explain (how things relate): morph, hierarchy, sequencing. Delight (use sparingly): stagger, spring, parallax, 3D flip.

## When not to animate

Frequent or repeated actions (keyboard navigation, typing, table row selection), long distances where travel time would hurt, anything the user triggers many times a minute, and when `motion = off` or the system asks for reduced motion.
