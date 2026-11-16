# Editing and error prevention

Read when the task affects editing and error prevention.

## Recipe

Problem: the task may become hard to understand or operate when this concern is ignored.

Technique: Keep labels, formats and constraints clear. Preserve entered data on errors and provide cancellation/undo where meaningful.

When to avoid or adapt: Premature validation can interrupt incomplete input; eligibility and network failures are not field-format errors.

Verification: Submit invalid input, correct it, retry a failed operation and confirm data is preserved.

## Adaptation and evidence

Use the actual framework and renderer APIs. DOM techniques apply to DOM renderers; desktop widgets require their own semantic/input mechanisms. Preserve applicable OS conventions and project identity. These are contextual heuristics unless the linked standard defines an applicable requirement.

[Source](https://design-system.service.gov.uk/patterns/validation/). Check the applicable version and scope before making a compliance claim.
