# Tables, lists and charts

Shared adaptation rules: [common](../common.md). The rules are self-contained; do not fetch the source to apply them.

## Rules

1. Right-align numbers, use tabular figures and a consistent number of decimals; left-align text; put units in the header.
2. Sortable headers show the current direction with an icon and expose it to assistive tech (web: `aria-sort`). Keep a stable secondary sort.
3. Virtualize long collections: render only visible rows (heuristic: consider it above a few hundred rows) and keep row height predictable.
4. Make rows keyboard operable: arrow keys move the active row, Enter opens, Space selects; show focus and selection separately.
5. Charts: bars start at zero, label directly instead of relying on a color legend, never use color alone to separate series (add shape, pattern or labels), and provide the data as a table or text summary.
6. Format dates, numbers and currency for the user's locale, never with a hard-coded pattern.
7. Show loading, empty and error states inside the table region so the header and layout stay put.

## Examples

- Bad: `1,234.5`, `12.25`, `3` stacked left-aligned. Good: `1,234.50`, `12.25`, `3.00` right-aligned with tabular figures.
- egui: `egui_extras::TableBuilder` renders only the visible rows (`body.rows(row_height, n, |row| ...)`) so 100 000 rows stay smooth.
- Bad: a line chart with a red and a green series. Good: add different markers and end-of-line labels, plus a "View data" table.
- Slint or Iced: wrap long lists in the toolkit's scroll and list-view widgets instead of building every child.

## Check

Load the largest realistic dataset and measure scroll smoothness; sort by each column; operate the table by keyboard; view a chart in grayscale; switch the locale.

[Source](https://www.w3.org/WAI/tutorials/tables/), kept for audit.
