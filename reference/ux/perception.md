# Perception and hierarchy

Shared adaptation rules: [common](../common.md). The rules are self-contained; do not fetch the source to apply them.

## Rules

1. Proximity groups. Space between related items is clearly smaller than space between groups (a ratio of about 1:2 or more); a label sits closer to its own field than to the previous one.
2. Build hierarchy with size, weight and contrast, and keep to about three emphasis levels per screen. If everything is bold, nothing is.
3. One primary action per view or dialog, visually dominant; secondary actions are quieter. Destructive actions are never the default focus.
4. Hick's law: decision time grows with the number of equal choices (about log2(n + 1)). Group, default and progressively disclose instead of showing 12 peer options.
5. Fitts's law: time to hit a target grows with distance and shrinks with size. Put frequent actions near the pointer or on screen edges and make them large; never put destructive buttons next to frequent ones.
6. Alignment: one left edge per column of content; align baselines of text in a row, not box tops.

## Examples

- Bad: a form where every label has 24 px above and below it, so labels float between fields. Good: 4 px between a label and its field, 20 px between field groups.
- Bad: "Save", "Save as", "Export", "Share", "Delete" as five equal filled buttons. Good: "Save" filled, "Save as" and "Export" in an overflow menu, "Delete" separated and styled as destructive.
- Bad: a settings page of 30 switches in one list. Good: 5 headed groups, the 3 most-changed settings first, advanced ones collapsed.

## Check

Squint test: blur the screenshot and confirm the primary action and the main grouping still show. Count equal-weight choices in each view.

## Adapt

Desktop shells (GNOME, COSMIC, Windows, macOS) define their own spacing and default-button conventions; follow them over the numbers above. [Source](https://www.nngroup.com/articles/gestalt-proximity/), kept for audit.
