# Visual system tactics

Shared rules: [common](../common.md), [index](index.md). Thresholds: [color](../ux/color.md), [typography](../ux/typography.md), [iconography](../ux/iconography.md).

### Semantic color roles
Use when: any themed UI. Do: define roles (background, surface, on-surface, primary, danger, warning, success, border, focus) and bind widgets to roles, never to hex values. Avoid: a literal color in a widget; it breaks dark and high-contrast themes.

### Accent restraint
Use when: choosing where color goes. Do: keep neutrals dominant and spend the accent on the primary action, selection and focus, roughly a 60 / 30 / 10 balance of neutral, secondary, accent. Avoid: coloring every control and heading.

### Dark mode
Use when: shipping a dark theme. Do: build it from its own role values, use slightly desaturated accents, express depth with lighter surfaces instead of shadows, and recheck every contrast pair. Avoid: pure inversion and pure black behind pure white text.

### Status colors
Use when: error, warning, success, info. Do: pair each with an icon and text, keep one color per meaning across the app, and check contrast in each theme. Avoid: color alone and using the danger color for non-errors.

### Color-blind safe palettes
Use when: charts, tags, status. Do: vary lightness and add shape, pattern or label; test with simulated deficiencies. Avoid: red versus green as the only difference.

### Type scale
Use when: setting text sizes. Do: define a small modular scale (for example 12, 14, 16, 20, 24, 32) with named roles (caption, body, title, headline) and a line height per role. Avoid: arbitrary sizes and more than about five levels per screen.

### Weight and size hierarchy
Use when: showing importance. Do: separate levels by size and weight together, keep body weight regular, and reserve bold for headings and key values. Avoid: all-caps paragraphs and color as the only emphasis.

### Font choice
Use when: identity or density needs it. Do: prefer the platform UI font, pair at most two families, use a monospace for code and IDs, and check glyph coverage for shipped scripts. Avoid: decorative fonts for body text.

### Tabular and numeric text
Use when: tables, prices, timers, counters. Do: use tabular figures, right alignment and consistent decimals so columns and changing values do not jitter. Avoid: proportional digits in live counters.

### Elevation and surfaces
Use when: layering menus, dialogs, cards. Do: use two to four elevation levels with consistent shadow or surface tint; higher means closer to the user (menus above dialogs above page). Avoid: shadows on everything and borders plus shadows plus fills on one element.

### Radius and borders
Use when: styling controls. Do: choose one or two corner radii, nest radii (inner = outer minus padding), keep border width consistent, and make borders reach 3:1 only where they identify a control. Avoid: mixing sharp and pill shapes without a reason.

### Icon system
Use when: any icons. Do: one family, one stroke weight, sizes of 16 and 24 px, theme-colored vectors, labels where meaning is not universal. Avoid: mixing filled and outline sets and scaling bitmaps.

### Design tokens
Use when: more than one screen or theme. Do: centralize color, spacing, radius, type, elevation and motion in tokens used by every component, and expose them to the toolkit's theme. Avoid: copying values between files.

### Theming and user preferences
Use when: respecting the system. Do: follow the OS light or dark setting, accent color and contrast settings by default, and let the user override. Avoid: ignoring forced colors and high-contrast modes.

### Focus visuals
Use when: keyboard use. Do: a clear focus ring with at least 3:1 contrast and a visible offset, styled with the same system as hover and selected but distinct. Avoid: removing the outline.

### Density and rhythm
Use when: a screen feels uneven. Do: keep a vertical rhythm from the spacing scale and equal row heights within a list. Avoid: mixed row heights without meaning.
