# Gesture and physics

Shared rules: [common](../common.md), [index](index.md). Direct manipulation has two phases. While the pointer is down, the element follows it exactly with no animation. On release, a physical model takes over from the current position and velocity.

### Spring
Communicates: weight and responsiveness. Use when: anything the user can interrupt or throw: sheets, pull to dismiss, drag release, toggles, resizable panes. Recipe: a spring with stiffness, damping and mass. Start near critical damping (damping ratio about 1) for professional UI: it settles without bouncing. Lower the ratio (about 0.6 to 0.8) for playful products. Springs have no fixed duration, so they stay natural when interrupted or retargeted. Avoid: bounce on error states and in dense tools.

### Inertia and fling
Communicates: momentum. Use when: scrolling, swiping a list, flicking a card. Recipe: keep the release velocity and decay it with friction until it stops or snaps. Avoid: custom inertia that disagrees with the platform's own scrolling feel.

### Rubber band
Communicates: a boundary. Use when: overscroll at list ends, drag past a limit. Recipe: movement beyond the limit is damped (for example a fraction of the pointer travel) and springs back on release. Avoid: hard stops that feel broken.

### Snap
Communicates: discrete states. Use when: carousels, bottom sheets with detents, sliders with steps. Recipe: pick the nearest detent using position plus a share of velocity, then spring to it. Avoid: snapping so strongly that fine control is lost.

### Velocity handoff
Communicates: continuity between gesture and animation. Use when: any release. Recipe: start the animation from the gesture's position and velocity so there is no jump at the handoff. Avoid: restarting from the resting position.

### Swipe to dismiss and reveal actions
Communicates: reversible removal. Use when: list rows, notifications. Recipe: follow the finger, reveal the action behind the row, commit past a threshold, and offer undo. Always provide a non-gesture alternative (menu or button) for accessibility. Avoid: gesture-only destructive actions.

### Pull to refresh and overscroll actions
Communicates: a deliberate action. Use when: touch-first lists with remote data. Recipe: a progress indicator that follows the pull, locks at a threshold and spins while loading. Avoid: on desktop where a refresh button and shortcut are expected.

### Draggable resize
Communicates: direct control. Use when: split panes, sidebars. Recipe: follow the pointer with no easing, animate only collapse and expand with a spring, and keep keyboard resize available. Avoid: animating content layout on every frame if it is heavy; update at the end or throttle.
