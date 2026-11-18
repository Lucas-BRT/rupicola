# Plan an interface

Inspect the product's tasks, relevant source, framework/version, targets and existing decisions. Identify hard requirements for accessibility, distribution, integration and data behavior. For an existing project, prefer its stack. For a new project, use reference/architecture.md to compare feasible environments before selecting a toolkit.

Define a reviewable functional slice: state transitions, input/error behavior, UI structure, dependencies actually needed, verification and a stopping condition. Include the few [tactics](../tactics/index.md) the slice needs, and a short motion plan (up to three tactics per screen) chosen from [motion library](../motion/index.md), unless the project sets `motion = off`. Explain material tradeoffs and assumptions. Ask only unresolved questions that change the result. Stop after delivering the plan; do not implement unless requested.
