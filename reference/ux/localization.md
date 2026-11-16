# International text and input

Read when the task affects international text and input.

## Recipe

Problem: the task may become hard to understand or operate when this concern is ignored.

Technique: Support expansion, plurals, locale-aware values, RTL where relevant, font fallback and IME composition.

When to avoid or adapt: Do not assume ASCII, concatenate translated sentence fragments or validate unfinished IME composition.

Verification: Try long translations, non-Latin names, locale formats and composition input without losing selection or data.

## Adaptation and evidence

Use the actual framework and renderer APIs. DOM techniques apply to DOM renderers; desktop widgets require their own semantic/input mechanisms. Preserve applicable OS conventions and project identity. These are contextual heuristics unless the linked standard defines an applicable requirement.

[Source](https://developer.apple.com/design/human-interface-guidelines/right-to-left). Check the applicable version and scope before making a compliance claim.
