# Data display tactics

Shared rules: [common](../common.md), [index](index.md). Thresholds: [data module](../ux/data.md).

### Table
Use when: comparing many records across several attributes. Do: right-align numbers with tabular figures, left-align text, units in headers, sticky header, column resize and reorder where useful, a visible sort indicator, row hover and keyboard focus. Avoid: tables for a handful of items and truncating key columns.

### Virtualized list
Use when: hundreds or more rows. Do: render only visible rows with stable row heights, keep scroll position and selection on update. Avoid: building every row and variable heights that jump.

### List and cards
Use when: items with media or mixed content, or scanning. Do: list for fast scanning, cards for browsing items with images; keep consistent card sizes and one primary action per card. Avoid: card grids for tabular data.

### Tree and hierarchy
Use when: nested data (files, folders, outlines). Do: expand and collapse with arrow keys, indent consistently, show counts, remember expansion. Avoid: unbounded nesting without search.

### Sort, filter and search
Use when: more than a screen of data. Do: show active filters as removable chips, a result count, a clear-all, saved views, and keep state in the URL or view. Avoid: filters whose effect is invisible.

### Pagination vs infinite scroll
Use when: long result sets. Do: pagination for goal-oriented tasks and when position matters, infinite scroll for feeds with a jump-to-top and a way to reach the footer. Avoid: infinite scroll with footer links and no position memory.

### Inline edit
Use when: frequent small changes. Do: click or Enter to edit, Escape cancels, Enter commits, show saving and failure states. Avoid: edits that silently fail.

### Selection model
Use when: acting on rows. Do: single, multiple and range selection with a visible count, select-all with a scope message ("all 1,240 results"), and an action bar. Avoid: selection that vanishes on page change without saying.

### Row actions
Use when: per-row commands. Do: one or two visible actions, the rest in an overflow menu, keyboard reachable, destructive separated. Avoid: hover-only actions.

### Chart choice
Use when: showing quantities. Do: line for trends over time, bar for category comparison, stacked only for part-to-whole with few parts, scatter for relationships, histogram for distribution, table when exact values matter. Avoid: pie with many slices, 3D, dual axes without need.

### Chart honesty
Use when: any chart. Do: bars start at zero, consistent scales across small multiples, label directly, show units and the time range, annotate events. Avoid: truncated axes that exaggerate.

### Chart accessibility
Use when: any chart. Do: do not rely on color alone, add patterns or markers, provide a data table and a text summary, make interactive points keyboard reachable. Avoid: charts as images with no alternative.

### Dashboard
Use when: monitoring. Do: put the most important number first, group by question, show comparison (delta, target, trend), consistent time range controls, drill-down links. Avoid: a wall of unrelated tiles.

### KPI tile
Use when: a headline number. Do: value, label, unit, comparison and a small trend line; explain the period. Avoid: decoration without a baseline.

### Timeline and logs
Use when: events over time. Do: newest-first or oldest-first consistently, group by time, level filters, search, "jump to latest" for live streams, preserve scroll when new items arrive. Avoid: content shifting under a reading user.

### Diff and code viewers
Use when: changes or source. Do: monospace, line numbers, side-by-side or inline toggle, word-level highlights, color plus symbols. Avoid: color-only add and remove.

### Formatting data
Use when: any number, date or unit. Do: locale-aware formats, consistent decimals, relative time with an absolute tooltip, units, null shown as an em dash or "None". Avoid: raw floats and ISO strings.

### Maps and spatial data
Use when: location matters. Do: zoom controls and keyboard panning, clustering, a list alternative, attribution. Avoid: map-only access to information.
