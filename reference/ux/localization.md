# Localization and internationalization

Shared adaptation rules: [common](../common.md). The rules are self-contained; do not fetch the source to apply them.

## Rules

1. Externalize every user-visible string into a catalog (gettext, Fluent or the toolkit's own) from the start; never leave literals in view code.
2. Plurals follow CLDR categories (zero, one, two, few, many, other), not `n == 1`. Use the catalog's plural syntax.
3. Allow text to grow: translations commonly run 30 to 50% longer than English, and much more for short labels (heuristic, not a standard). No fixed-width buttons or clipped labels.
4. Support right-to-left layouts: mirror layout and directional icons, keep numbers and Latin runs in their own direction.
5. Format dates, times, numbers, currency and units with the locale (ICU data), not manual patterns; sort with locale collation.
6. Check font coverage for every shipped script (CJK, Arabic, Devanagari) and use fallback fonts rather than tofu boxes.
7. Input methods: do not act on key presses during IME composition; the text field must show the preedit string.

## Examples

- Rust crates: `fluent` and `fluent-bundle` for messages with plurals and selectors; `gettext-rs` for gettext catalogs; ICU4X (`icu`) for dates, numbers and collation. Pick the one the project already uses.
- Bad: `format!("{} items", n)` shown as "1 items". Good: a Fluent message with plural variants:

```ftl
items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
```

- Bad: `Button::new("Save")` with a 90 px width. Good: width from content with a minimum, tested with "Speichern" and a pseudo-locale like "[!!! Šàvé !!!]".
- Bad: shortcuts bound to `S` only. Good: shortcuts that use modifier chords, which do not clash with IME or non-Latin layouts.

## Check

Run the app with a pseudo-locale (long, accented), a right-to-left locale and a CJK locale; type with an IME; confirm nothing clips, flips wrongly or shows missing glyphs.

[Source](https://cldr.unicode.org/index/cldr-spec/plural-rules), kept for audit.
