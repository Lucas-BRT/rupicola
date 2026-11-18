# Localization tactics

Shared rules: [common](../common.md), [index](index.md). Thresholds: [localization module](../ux/localization.md).

### String externalization
Use when: always, from the first screen. Do: keep all user text in catalogs (Fluent, gettext or the toolkit's own) with stable ids and translator comments. Avoid: literals in view code.

### Plurals and selectors
Use when: counts and gendered or case-dependent text. Do: use catalog plural categories (CLDR) and selectors with named variables. Avoid: `n == 1` branches and string assembly.

### Text expansion
Use when: sizing controls. Do: let controls grow with content, set minimums not fixed widths, allow wrapping, test with a pseudo-locale about 30 to 50% longer, more for short labels. Avoid: fixed-width buttons and single-line clipping.

### Right-to-left
Use when: Arabic, Hebrew, Persian, Urdu. Do: mirror layout, alignment, directional icons and progress, keep numbers and Latin runs in their own direction, use logical start and end instead of left and right. Avoid: mirroring clocks, logos and media controls.

### Bidirectional text
Use when: mixed scripts, user content. Do: isolate user-provided strings, set direction per text run, test with mixed input. Avoid: punctuation jumping to the wrong side.

### Formats
Use when: dates, times, numbers, currency, units, lists, names. Do: format with locale data (ICU), keep time zones explicit, localize list separators and name order. Avoid: hand-written patterns.

### Sorting and search
Use when: lists and search. Do: use locale-aware collation and case and accent-insensitive matching where appropriate. Avoid: byte-order sorting of names.

### Fonts and scripts
Use when: shipping beyond Latin. Do: verify glyph coverage, set fallback fonts, adjust line height for scripts with tall glyphs, avoid letter-spacing on connected scripts. Avoid: tofu boxes.

### Input methods
Use when: CJK and other IME users. Do: show the preedit text, do not act on keys during composition, keep shortcuts chord-based. Avoid: single-key shortcuts that clash with IME.

### Pseudo-localization
Use when: testing early. Do: run a pseudo-locale with accents, expansion and brackets to find clipping, concatenation and missing strings. Avoid: leaving it until translation arrives.

### Culture-neutral content
Use when: icons, colors, examples, images. Do: avoid idioms, culture-specific gestures and symbols, color meanings assumed universal, and example data tied to one place. Avoid: jokes that do not translate.

### Gender and inclusive language
Use when: addressing the user. Do: prefer neutral phrasing, avoid assuming gender, and support grammatical gender through selectors where the language needs it. Avoid: hard-coded pronouns.

### Locale switching
Use when: language settings. Do: honor the system language, allow an in-app override, apply without restart where possible, remember the choice. Avoid: language chosen from IP.
