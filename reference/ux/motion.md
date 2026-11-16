# Continuity and input feedback

Read when the task affects continuity and input feedback.

## Recipe

Problem: the task may become hard to understand or operate when this concern is ignored.

Technique: Animate only for a purpose. Relate popover motion to its trigger, preserve continuity and allow interruption. Respect reduced motion.

When to avoid or adapt: Repeated keyboard selection should not wait for decorative movement. Springs and durations depend on task, distance and toolkit.

Verification: Repeat the interaction rapidly, interrupt transitions and enable reduced motion; verify input and focus remain correct.

## Adaptation and evidence

Use the actual framework and renderer APIs. DOM techniques apply to DOM renderers; desktop widgets require their own semantic/input mechanisms. Preserve applicable OS conventions and project identity. These are contextual heuristics unless the linked standard defines an applicable requirement.

[Source](https://emilkowal.ski/ui/you-dont-need-animations). Check the applicable version and scope before making a compliance claim.
