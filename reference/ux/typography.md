# Reading and comparable information

Read when the task affects reading and comparable information.

## Recipe

Problem: the task may become hard to understand or operate when this concern is ignored.

Technique: Use semantic text roles, readable line length and spacing. Align comparable numbers and retain locale-aware formatting.

When to avoid or adapt: One fixed font size or exactly two weights is not a universal requirement.

Verification: Enlarge text and test long translations; inspect clipping, fallback glyphs and tabular alignment.

## Adaptation and evidence

Use the actual framework and renderer APIs. DOM techniques apply to DOM renderers; desktop widgets require their own semantic/input mechanisms. Preserve applicable OS conventions and project identity. These are contextual heuristics unless the linked standard defines an applicable requirement.

[Source](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html). Check the applicable version and scope before making a compliance claim.
