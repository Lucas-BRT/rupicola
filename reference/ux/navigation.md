# Navigation

Shared adaptation rules: [common](../common.md). The rules are self-contained; do not fetch the source to apply them.

## Rules

1. The user can always tell where they are: highlight the current destination and show a title or breadcrumb for deep content.
2. Keep navigation in the same place and order on every screen; do not reorder items by usage.
3. Pick the pattern by size of the destination set: tabs for 2 to 5 peer views of the same object, a sidebar for about 5 to 15 top-level destinations, search or a command palette beyond that. These counts are heuristics.
4. Back behaves predictably: it returns to the previous state, with scroll position and entered data kept. A modal never replaces the main navigation stack.
5. Primary destinations are reachable by keyboard (Tab order, shortcuts, arrow keys inside tab lists and sidebars) and exposed with proper roles.
6. On desktop widths do not hide primary navigation behind a hamburger menu; collapse it only when the window is narrow.

## Examples

- Bad: opening "Settings" as a full-screen replacement with no way to know how to return. Good: a titled page or dialog with a visible Back or Close and Escape support.
- Bad: tabs that jump when a badge appears. Good: reserve badge space so labels do not shift.
- Web or Leptos: a tab strip uses `role="tablist"`, `role="tab"` with `aria-selected`, and arrow keys move between tabs.
- libadwaita: `adw::NavigationSplitView` for sidebar plus content that collapses on narrow windows.

## Check

Reach every top-level destination with the keyboard only; navigate back and forward and confirm state is restored; resize narrow and confirm navigation is still reachable.

## Adapt

Follow the platform: GNOME and COSMIC favor sidebars and header bars, macOS toolbars and sidebars, Windows navigation views. [Source](https://www.nngroup.com/articles/navigation-ia-tests/), kept for audit.
