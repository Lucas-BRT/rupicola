# Truthful states and recovery

Read when the task affects truthful states and recovery.

## Recipe

Problem: the task may become hard to understand or operate when this concern is ignored.

Technique: Distinguish loading, empty, failed and offline results. Show honest progress and useful recovery actions.

When to avoid or adapt: A spinner is not proof of progress; optimistic success needs a recovery strategy if the operation fails.

Verification: Exercise slow, failed, canceled and successful operations; compare displayed state to actual outcome.

## Adaptation and evidence

Use the actual framework and renderer APIs. DOM techniques apply to DOM renderers; desktop widgets require their own semantic/input mechanisms. Preserve applicable OS conventions and project identity. These are contextual heuristics unless the linked standard defines an applicable requirement.

[Source](https://www.nngroup.com/articles/ten-usability-heuristics/). Check the applicable version and scope before making a compliance claim.
