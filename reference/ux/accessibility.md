# Operating with diverse abilities

Read when the task affects operating with diverse abilities.

## Recipe

Problem: the task may become hard to understand or operate when this concern is ignored.

Technique: Provide meaningful roles, names, states, focus order and keyboard alternatives. Respect text scaling and motion preferences.

When to avoid or adapt: An accessibility library or clean screenshot does not prove the widget tree works with assistive technology.

Verification: Inspect semantics and operate the flow with keyboard and available screen reader; report unavailable checks.

## Adaptation and evidence

Use the actual framework and renderer APIs. DOM techniques apply to DOM renderers; desktop widgets require their own semantic/input mechanisms. Preserve applicable OS conventions and project identity. These are contextual heuristics unless the linked standard defines an applicable requirement.

[Source](https://www.w3.org/WAI/WCAG22/). Check the applicable version and scope before making a compliance claim.
