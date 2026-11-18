# Design direction tactics

Shared rules: [common](../common.md), [index](index.md). These are advisory: a style choice is not a defect on its own, and project identity wins over these suggestions. See [visual system](visual-system.md) for tokens and [motion library](../motion/index.md) for movement.

### Name the direction first
Use when: starting a screen or restyling. Do: write one line for the audience, the task, the tone (calm, dense, playful, technical) and one memorable trait; choose all later decisions against it. Avoid: starting from a component kit's defaults.

### Match the product category
Use when: choosing density and personality. Do: tools for long sessions favor density, calm color and speed; consumer apps favor warmth and clarity; dashboards favor hierarchy and comparison. Avoid: marketing-page styling in a work tool.

### Reduce before decorating
Use when: a screen looks plain or noisy. Do: fix hierarchy, spacing and alignment before adding color, shadow or illustration. Avoid: effects as a substitute for structure.

### Scale contrast
Use when: creating hierarchy. Do: make the biggest element clearly bigger than the next and the smallest clearly legible; use few sizes with decisive jumps. Avoid: many sizes within a narrow range.

### One signature element
Use when: giving the product character. Do: pick one distinctive move (a typographic treatment, a color, a shape language, a motion trait) and apply it consistently. Avoid: five competing flourishes.

### Composition and asymmetry
Use when: layouts feel templated. Do: use deliberate asymmetry, anchored edges and varied block sizes where content allows, and keep dense tools regular. Avoid: identical cards in a uniform grid for unlike content.

### Typographic voice
Use when: identity matters. Do: choose a typeface whose character fits the tone, use weight and size contrast, set numbers tabular. Avoid: defaulting every product to the same system look when identity is wanted.

### Color moves
Use when: building a palette. Do: start from neutrals with a slight tint toward the accent, keep one accent, and make status colors distinct from the accent. Avoid: purple-to-blue gradients by default and gradient text.

### Depth and texture
Use when: surfaces need presence. Do: use elevation and subtle borders consistently, add texture or imagery with purpose and checked contrast. Avoid: glass effects everywhere and heavy shadows.

### Imagery and illustration
Use when: empty states, onboarding, marketing. Do: one illustration style, simple, tied to the product, with a text equivalent. Avoid: stock imagery and emoji as icons.

### Personality in small places
Use when: the product needs warmth. Do: put character in empty states, success moments, loading text and microcopy, never in errors or destructive flows. Avoid: jokes when the user is blocked.

### State coverage polish
Use when: finishing. Do: draw every control in rest, hover, focus, pressed, selected, disabled and error, in light and dark. Avoid: styling only the rest state.

### Optical corrections
Use when: details look slightly off. Do: adjust round shapes and icons optically, align text and icon baselines, keep consistent radii and borders, snap to pixels at common scales. Avoid: mathematically aligned but visually off elements.

### Anti-pattern checklist
Use when: reviewing. Do: look for these, then ask whether each helps the task: generic card grids with soft shadows, gradient text, glass everywhere, oversized radii, centered hero plus three feature cards, emoji icons, uniform spacing, lorem ipsum, identical hover effects, unrequested animation. Avoid: treating the presence of one as a confirmed defect.

### Critique at three scales
Use when: evaluating a screen. Do: look at it blurred for hierarchy, at normal size for alignment and rhythm, and zoomed for details and states; compare light and dark. Avoid: judging from one screenshot.

### Evidence from the project
Use when: unsure what fits. Do: read existing screens, tokens and brand notes in the project and match them; ask only when the project gives no signal. Avoid: imposing a new look on an established product.
