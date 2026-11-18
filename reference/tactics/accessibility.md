# Accessibility tactics

Shared rules: [common](../common.md), [index](index.md). Thresholds: [accessibility module](../ux/accessibility.md), [input](../ux/input.md), [color](../ux/color.md).

### Native semantics first
Use when: any control. Do: use the toolkit's real button, checkbox, list and text field, which carry role, name, state and keyboard behavior; add ARIA or accessible properties only to fill gaps. Avoid: custom-drawn controls with no accessibility tree.

### Accessible names
Use when: icon buttons, inputs, regions. Do: give each control a name that matches its visible label (so voice control can say it), plus a description when needed. Avoid: names like "button" and icons with no name.

### Keyboard path
Use when: every flow. Do: all actions reachable by keyboard in a logical order, visible focus, standard keys (Space, Enter, Escape, arrows), no traps. Avoid: pointer-only actions.

### Focus management
Use when: dialogs, popovers, inline editing, route changes. Do: move focus in on open, trap it in modals, return it on close, focus the first error on submit, keep logical position after updates. Avoid: focus lost to the top.

### Live announcements
Use when: status changes without focus change (saved, results count, errors). Do: announce through the toolkit's live-region or announcement API, politely unless urgent. Avoid: announcing every frame or every keystroke.

### Text scaling and zoom
Use when: any text. Do: honor system text size, layouts that reflow, no fixed text heights, test at 200%. Avoid: pixel-locked containers.

### Contrast and non-text contrast
Use when: any color. Do: text 4.5:1, large text and UI parts 3:1, focus indicator 3:1, in every theme. Avoid: low-contrast placeholders and borders on inputs.

### Forced colors and high contrast
Use when: supporting system contrast modes. Do: use system colors in those modes, keep borders and focus visible, do not rely on background images. Avoid: hard-coded colors that vanish.

### Reduced motion and transparency
Use when: any animation or blur. Do: honor reduced motion and reduced transparency settings, replace movement with fades or cuts. Avoid: parallax and auto-playing loops.

### Cognitive load
Use when: complex tasks. Do: plain language, one task per step, consistent navigation and terms, visible progress, the ability to review and correct. Avoid: time limits without extension and surprise changes.

### Forgiving input
Use when: forms and commands. Do: accept varied formats, explain errors specifically, allow undo, keep input after failure. Avoid: strict formats and wiping fields.

### Authentication without memory tests
Use when: sign-in. Do: allow paste and password managers, offer alternatives to puzzles and recall (WCAG 3.3.8). Avoid: CAPTCHAs without an alternative.

### Pointer accommodations
Use when: touch and mouse. Do: target size at least 24 x 24 px, spacing, single-pointer alternatives to drag and path gestures, cancel by moving away before release. Avoid: tiny clustered targets.

### Captions and alternatives
Use when: audio, video, images, charts. Do: captions and transcripts for media, text alternatives for meaningful images and charts, none for decoration. Avoid: information only in sound or image.

### Screen reader testing
Use when: verifying. Do: run the flow with the platform screen reader (Orca, NVDA or Narrator, VoiceOver) and inspect the tree; record what was not tested. Avoid: claiming conformance from a clean screenshot.
