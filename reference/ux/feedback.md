# Feedback, errors and empty states

Shared adaptation rules: [common](../common.md). The rules are self-contained; do not fetch the source to apply them.

## Rules

1. Response-time thresholds (Nielsen): about 0.1 s feels instant, about 1 s keeps the flow of thought, about 10 s is the limit of attention. Acknowledge input immediately; show progress for anything over about 1 s.
2. Use determinate progress when the amount is known, an indeterminate spinner only for short unknown waits, and a cancel button for long ones.
3. Errors say what happened, why if known, and how to fix it, next to the cause. Keep what the user typed.
4. Prefer undo over confirmation for reversible actions. Confirm only irreversible ones, and name the action on the button ("Delete 3 files"), never "OK".
5. Toasts are for non-critical confirmations. They must not carry critical errors or required actions, and any timing must be adjustable or long enough (WCAG 2.2.1); announce them to assistive tech (4.1.3).
6. Every empty state explains why it is empty and offers the next action. A blank panel is a defect.
7. Avoid false states: never show "Saved" before the operation succeeds; reject or ignore stale async results.

## Examples

- Bad: "Error 0x80070057". Good: "Could not save report.pdf: the disk is full. Free some space or choose another folder." with a Choose folder button.
- Bad: a list screen that is blank after a failed load. Good: "Could not load entries. Check your connection." with a Retry button; and for a successful empty list, "No entries yet. Add your first one." with an Add button.
- Bad: dialog "Are you sure? [OK] [Cancel]". Good: "Delete 3 files permanently? [Cancel] [Delete 3 files]" with Cancel as default focus.
- egui: keep `load_error: Option<String>` and `entries: Vec<_>` in state, render the error with a retry button, then the empty message, then the list.

## Check

Force the failure, the empty case, a slow response and a double-submit; confirm each shows the right message, keeps input and cannot show a stale result.

[Source](https://www.nngroup.com/articles/response-times-3-important-limits/), kept for audit.
