# Visual critique

Shared rules: [common](../reference/common.md), [visual verification](visual-verification.md). Use this when judging a screenshot or a set of them. A critique is only useful when it can be acted on and checked.

## Finding format

One finding per line of reasoning, each with:

- Where: an element id or a region (x, y, width, height) in a named image.
- Dimension: hierarchy, alignment, spacing rhythm, contrast, density, consistency, state coverage, motion, or brand fit.
- Severity: blocker (task fails or content unreadable), major (clear friction), minor (noticeable), polish (taste).
- Evidence: measured, seen, opinion or unverified (see [visual verification](visual-verification.md)).
- Fix: a specific change a developer can make, with the target value when it is a number.

Rank by severity, then by how many screens it affects. Do not list a style preference as a defect: a hue, card or animation alone is not a confirmed problem.

## What each source is good for

Measured numbers settle contrast, target size, names, clipping and overlap. A reading of the image fits hierarchy, rhythm, balance and consistency. Interaction-dependent judgments (is the flow clear, is feedback fast enough, does focus move sensibly) need the running app, so mark them "needs an interactive check" instead of concluding from a still.

## Rubric

- Hierarchy: the primary action and the main content are found within a second of squinting; no more than about three emphasis levels.
- Alignment and rhythm: one left edge per column, consistent gaps from the spacing scale, no 1 to 3 px near-misses.
- Contrast and legibility: text and boundaries meet the thresholds in every theme; text survives 200% size.
- Density and scanning: rows and groups are separable; data lines up; nothing clips.
- Consistency: the same thing looks the same everywhere; components and tokens are reused.
- State coverage: ideal, empty, loading, error, long text, narrow window and right-to-left were each viewed.
- Motion: tweens rather than cuts, nothing blocks input, reduced motion works.
- Fit: it matches the project's identity and the platform's conventions.

## Before and after

For an improvement, show the before and after of the same state and size with `rupicola diff`. Explain every changed region outside the edit as intended or as a regression.
