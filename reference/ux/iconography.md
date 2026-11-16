# Recognizable actions

Read when the task affects recognizable actions.

## Recipe

Problem: the task may become hard to understand or operate when this concern is ignored.

Technique: Reuse a coherent icon set and label unfamiliar or icon-only actions accessibly. Align by optical weight where geometric centering looks wrong.

When to avoid or adapt: A decorative icon needs no redundant announcement; a tooltip alone may not make an action discoverable.

Verification: Inspect accessible names and ask whether the action remains understandable without hovering.

## Adaptation and evidence

Use the actual framework and renderer APIs. DOM techniques apply to DOM renderers; desktop widgets require their own semantic/input mechanisms. Preserve applicable OS conventions and project identity. These are contextual heuristics unless the linked standard defines an applicable requirement.

[Source](https://developer.gnome.org/hig/guidelines/ui-icons.html). Check the applicable version and scope before making a compliance claim.
