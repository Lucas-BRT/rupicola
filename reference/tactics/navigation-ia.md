# Navigation and information architecture tactics

Shared rules: [common](../common.md), [index](index.md). Thresholds: [navigation module](../ux/navigation.md).

### Sidebar
Use when: about 5 to 15 stable top-level areas, desktop widths. Do: persistent list with icons and labels, a clear current item, group headings, collapse to overlay when narrow. Avoid: a hamburger on wide windows.

### Tabs
Use when: two to five peer views of the same object. Do: tab list with arrow keys, a selected state beyond color, and preserve each tab's state. Avoid: tabs for sequential steps and tab overflow without a menu.

### Breadcrumbs and path
Use when: deep hierarchies. Do: show the path with links, collapse the middle when long, keep the current page unlinked. Avoid: breadcrumbs for flat sites.

### Header bar and title
Use when: any window. Do: a title that names the current place, primary actions on the right or by platform rule, back at the left in drill-down. Avoid: empty bars.

### Menus
Use when: many actions. Do: a menu bar or app menu for global actions, context menus for object actions, an overflow menu for the rest; group, separate destructive items, show shortcuts. Avoid: deep submenus and unlabeled icon menus.

### Context menus
Use when: acting on an item. Do: open on right-click, long-press and keyboard (menu key, Shift+F10), mirror the item's visible actions. Avoid: actions only in a context menu.

### Search
Use when: more than a screen of content. Do: a visible search with shortcut, results grouped by type, highlight matches, remember recent queries, show a useful no-results state. Avoid: search that only matches exact names.

### Command palette
Use when: power users and many commands. Do: a global fuzzy launcher listing destinations and actions with shortcuts. Avoid: hiding discoverable commands behind it.

### Wayfinding
Use when: the user may lose their place. Do: highlight the current location, show titles and counts, keep navigation order stable, preserve scroll and selection on return. Avoid: reordering items by usage.

### Back and history
Use when: drill-down and modals. Do: Back returns to the previous state with data and scroll kept; Escape closes overlays; offer back and forward for document-like views. Avoid: replacing the navigation stack with a modal.

### Deep links and restore
Use when: apps users reopen. Do: restore the last view, window size and selection; let users share a link to a place. Avoid: always opening on the home view.

### Information architecture
Use when: organizing many options. Do: group by the user's tasks, name groups in their words, test the structure with a card sort or task walk-through, keep depth shallow. Avoid: organizing by the code's modules.

### Settings organization
Use when: a settings area. Do: group by task, put common settings first, apply immediately where safe, show restart-needed changes clearly, offer search and a reset. Avoid: one flat list of dozens of switches.

### Wizards and onboarding flow
Use when: first run or complex setup. Do: ask only what is needed to start, allow skipping, show progress, finish on a useful screen. Avoid: forced tours before first use.

### Discoverability
Use when: features that are hard to find. Do: show shortcuts in menus and tooltips, use empty states that teach, add a command palette entry, and use a "what's new" note once. Avoid: modal tutorials on every update.

### Multiple windows and tabs
Use when: workflows spanning documents. Do: let users open items in a new window or tab, remember layout, and keep shared state consistent. Avoid: opening a new window for routine navigation.
