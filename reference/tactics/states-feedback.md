# States and feedback tactics

Shared rules: [common](../common.md), [index](index.md). Thresholds: [feedback module](../ux/feedback.md). Movement for these states: [motion library](../motion/feedback-loading.md).

### The state set
Use when: any screen with data or an action. Do: design each of ideal, empty, loading, partial, error, offline, disabled, permission-denied and success; check each in the build. Avoid: only the happy path.

### Empty state
Use when: no data yet or no results. Do: say why it is empty, what the area is for, and give one primary action (add, import, clear filters). Avoid: a blank panel, or a joke instead of a next step.

### No-results state
Use when: search or filter returns nothing. Do: restate the query, offer to clear filters or broaden, suggest alternatives. Avoid: the same message as a first-run empty state.

### Loading state
Use when: any wait. Do: acknowledge input at once, show skeletons for known layouts, determinate progress for known amounts, spinner only after a short delay, and a cancel for long work. Avoid: blocking the whole window for a local wait.

### Error anatomy
Use when: any failure. Do: say what happened, why if known, how to fix it, and give the action (Retry, Choose another folder); place it by the cause and keep the user's input. Avoid: codes, blame and generic "Something went wrong".

### Partial failure
Use when: batch operations. Do: report how many succeeded and failed, list failures, offer to retry only those. Avoid: all-or-nothing messages.

### Offline and sync
Use when: network dependent apps. Do: show connection and sync status, queue changes, say what is saved locally, and resolve conflicts by showing both versions. Avoid: silent data loss or silent overwrite.

### Success confirmation
Use when: an action completed. Do: the lightest sufficient signal (inline change, toast, status text), announce it to assistive tech. Avoid: modal "Success" dialogs.

### Notification ladder
Use when: telling the user something. Do: pick by urgency: inline text, badge, toast (non-critical, auto-dismiss, with undo), banner (persistent, needs attention), dialog (blocks, requires a decision). Avoid: critical errors in a toast.

### Toast rules
Use when: transient messages. Do: keep long enough to read, allow dismissal and pause on hover or focus, include one action at most, announce as a status. Avoid: stacking many and hiding required actions.

### Confirmation dialogs
Use when: irreversible, costly or surprising actions only. Do: title states the decision, body states the consequence, buttons name the action, safe action is the default. Avoid: confirming reversible actions; prefer undo.

### Undo and redo
Use when: edits, deletes, moves. Do: a visible undo for a short window and standard shortcuts, a history for complex documents. Avoid: undo that works only sometimes without saying.

### Preview and dry run
Use when: bulk or destructive operations. Do: show exactly what will change before applying, with counts. Avoid: applying then explaining.

### Disabled controls
Use when: an action is unavailable. Do: explain why next to the control or in a tooltip reachable by keyboard, or hide when never possible. Avoid: unexplained grayed-out controls.

### Permission and capability prompts
Use when: needing access (files, camera, notifications). Do: ask in context at the moment of need, explain the benefit first, handle denial with a clear path to change it. Avoid: asking on launch.

### Safe defaults and guardrails
Use when: risky actions. Do: default to the safe choice, rate-limit, require extra intent for destructive bulk actions (type the name), keep an audit trail where relevant. Avoid: one-click irreversible deletion.

### Recovery paths
Use when: crash, corruption, mistaken action. Do: autosave, offer recovery on restart, keep a trash for deletes. Avoid: telling users to start over.

### System status visibility
Use when: long or background work. Do: show what is running, how far it is, and where results will appear, and let it continue in the background. Avoid: silent background activity.
