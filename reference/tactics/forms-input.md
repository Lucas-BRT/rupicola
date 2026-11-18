# Forms and input tactics

Shared rules: [common](../common.md), [index](index.md). Thresholds: [input](../ux/input.md), [feedback](../ux/feedback.md).

### Visible labels
Use when: every field. Do: a persistent label above or beside the field, tied to it programmatically; use placeholders only for example format. Avoid: placeholder-as-label and labels that vanish on input.

### Validation timing
Use when: any validated field. Do: validate on blur or submit for first entry, then live while correcting; show the message beside the field and keep the entered value. Avoid: errors while the user is still typing the first time and clearing the field on error.

### Error summary
Use when: long forms or several errors on submit. Do: list errors at the top with links that move focus to each field, and mark each field inline too. Avoid: only a toast, and errors that do not say which field.

### Smart defaults
Use when: a choice has a usual answer. Do: preselect it, prefill from known data or system locale, and keep it changeable. Avoid: defaults that opt the user into sharing or charges.

### Input types and constraints
Use when: structured values. Do: use the right control (date picker, number stepper, slider with value, combobox, segmented control, switch for immediate effect), set constraints, and allow keyboard entry in pickers. Avoid: a free text box for dates and sliders without a numeric value.

### Autocomplete and suggestions
Use when: long lists, addresses, tags, commands. Do: filter as the user types, highlight the match, support arrow keys and Enter, allow free text when valid, announce result counts. Avoid: forcing selection when the value may be new.

### Input masks and formatting
Use when: phone, card, ID, currency. Do: accept pasted and unformatted input, format after the fact, never reject separators the user typed. Avoid: masks that fight the caret.

### Progressive forms and steppers
Use when: a long process. Do: split into a few labeled steps with progress, allow Back without losing data, show a summary before commit. Avoid: wizards for four fields.

### Conditional fields
Use when: a field matters only after another answer. Do: reveal it beside the trigger without shifting focus, and keep entered values if hidden then shown again. Avoid: hiding required data.

### Required and optional marking
Use when: mixed forms. Do: mark the minority in text ("Optional") instead of only asterisks. Avoid: color-only markers.

### Draft saving and recovery
Use when: long text or multi-step work. Do: autosave drafts locally, restore on return, and tell the user. Avoid: losing input on navigation, timeouts or crashes.

### Secure fields
Use when: passwords and secrets. Do: allow paste and password managers, offer show or hide, give clear rules before the user fails, no forced composition rules beyond need. Avoid: blocking paste.

### Undo over confirmation
Use when: reversible actions. Do: perform immediately and offer undo for a while. Avoid: "Are you sure?" on common actions.

### Keyboard shortcuts
Use when: frequent actions. Do: use platform-standard shortcuts, show them in menus and tooltips, use modifier chords for global ones. Avoid: single-letter global shortcuts and clashes with the OS or IME.

### Command palette and search
Use when: many commands or destinations. Do: one shortcut opens a fuzzy search over actions, show shortcuts beside results, recent first. Avoid: making it the only path to a command.

### Bulk and multi-select
Use when: acting on many items. Do: checkboxes with shift-range and select-all, a visible count, an action bar that appears on selection, undo. Avoid: hidden selection state.

### Drag and drop
Use when: files, ordering. Do: show a drop target, accept multiple items, give a keyboard or menu alternative. Avoid: drag as the only path.

### Pointer and gesture alternatives
Use when: swipe, long-press, hover. Do: give every gesture a visible control, and never require hover. Avoid: hidden actions that only power users find.

### Focus management
Use when: dialogs, inline edits, validation. Do: move focus into a dialog and return it on close, focus the first error, keep focus on the same logical item after a list updates. Avoid: losing focus to the page top.
