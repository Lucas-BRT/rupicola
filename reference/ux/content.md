# Interface text

Shared adaptation rules: [common](../common.md). The rules are self-contained; do not fetch the source to apply them.

## Rules

1. Buttons start with a specific verb and name the object: "Save changes", "Delete account", "Add member". Avoid "OK", "Submit", "Click here".
2. A label says what the field is, in the same words used elsewhere. Pick one term per concept (do not alternate "project", "workspace" and "folder").
3. Write for the task, not the implementation: no internal error codes, enum names or Rust type names in user text.
4. Use sentence case consistently, a single punctuation style and the active voice. Keep dialogs to a title that states the decision and a short body.
5. Placeholders show an example format, never the label (WCAG 3.3.2); required and optional are marked in text.
6. Truncate with an ellipsis only when the full text is available (tooltip, wrap or detail view), and never truncate an error message.
7. Never concatenate sentence fragments; use whole messages with named placeholders so translators can reorder words.

## Examples

- Bad: button "OK" on a dialog titled "Warning". Good: title "Discard unsaved changes?", buttons "Keep editing" and "Discard changes".
- Bad: label "Name" in the signup and "Full name" in the profile for the same field. Good: "Full name" in both.
- Bad: `format!("Deleted {} " + if n == 1 {"file"} else {"files"})`. Good: a catalog message with plural variants (Fluent):

```ftl
deleted-files = { $count ->
    [one] Deleted one file
   *[other] Deleted { $count } files
}
```

- Bad: "Invalid input". Good: "Enter an email address like name@example.com".

## Check

Read every button and error out of context: does each say what will happen? Search for repeated concepts with different names and for strings built with `format!` plus plural logic.

[Source](https://www.w3.org/WAI/WCAG22/Understanding/labels-or-instructions.html), kept for audit.
