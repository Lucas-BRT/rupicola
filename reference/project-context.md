# Project context

Use `.rupicola/context.md` for explicit project-context requests. Record product tasks, packages, resolved dependency versions or uncertainty, features, renderers, OS targets, brand decisions, accepted exceptions and verification evidence. Distinguish user decisions from inferred assumptions and date volatile facts.

Read existing context when it participates in the current task. Do not create context files during ordinary small edits unless requested. Preserve existing user records; this skill does not automatically migrate or delete other tools' state directories.

Optional `proactivity: off | propose | apply` controls the [tactics library](tactics/index.md): `off` suggests nothing, `propose` lists tactics without applying them, `apply` also implements the ones the task's own states require (the default for build; improve and review propose).

Optional `browser_support: baseline-widely | baseline-newly | latest` sets how new a web platform feature may be before it needs a fallback (see [modern web features](web/modern-css.md); default `baseline-widely` with enhancements behind feature checks).

Optional `motion: off | subtle | expressive` sets how much animation the [motion library](motion/index.md) may add (default `subtle`; `off` disables it).

Optional `visits: enabled` allows review records under `.rupicola/visits/`. Otherwise return the report without automatically persisting it. Store reusable decisions rather than copied manuals, transcripts or tool dumps.
