# Plan an interface

Inspect the product's tasks, relevant source, framework/version, targets and existing decisions. Identify hard requirements for accessibility, distribution, integration and data behavior. For an existing project, prefer its stack. For a new project, use reference/architecture.md to compare feasible environments before selecting a toolkit.

Define a reviewable functional slice: state transitions, input/error behavior, UI structure, dependencies actually needed, verification and a stopping condition. Include the few [tactics](../tactics/index.md) the slice needs, and a short motion plan (up to three tactics per screen) chosen from [motion library](../motion/index.md), unless the project sets `motion = off`. Explain material tradeoffs and assumptions. Stop after delivering the plan; do not implement unless requested.

## Goal-driven technical discovery

When the user states a broad goal, new interface, or open-ended redesign rather than a narrow surgical fix:

1. **Ground feasibility first:** inspect the workspace (`discover`, `design open`, dependencies, renderer, CSP/platform limits) before asking questions so every option you offer is technically viable in the actual stack.
2. **Interview before assuming (1 round, 4–6 closed questions):** use the host's structured question tool (`ask_question`) when available—otherwise a concise numbered list—to ask 4–6 high-leverage technical questions in a single round (skip for narrow bugfixes or already-specified tasks):
   - **Task & density:** daily power-tool density vs guided low-frequency flow.
   - **Targets & input:** required viewports (`1440px` + `320px`), keyboard-first shortcuts vs coarse touch targets.
   - **Visual & content rules:** aesthetic direction, `oklch()` neutral temperature, typography roles, radius/hairline system, and terse copy budgets.
   - **State & persistence:** URL query-param state vs server DB vs local storage; optimistic vs confirmed mutations; duplicate-submit guards.
   - **Data extremes & recovery:** behavior at `0`, `1`, and `100+` items, long strings, and network/validation failures.
   - **Verification target:** headless Chromium, offscreen RAM (`RamCapture`), or compositor capture.
3. **Persist decisions:** record the user's answers as `adopted` entries via `rupicola design add` so subsequent build/review loops honor them without re-asking.
