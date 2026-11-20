# Evidence-based UI reports

Report the requested package, tasks, framework/version, renderer and target OS first. Separate confirmed runtime defects from static candidates and unknowns. Rank blocked tasks, data loss, false status and inaccessible controls before aesthetics. Every finding needs location, affected user/task, evidence and a concrete correction.

For code-only reviews, state that input behavior, assistive technology and runtime performance remain unverified. Do not derive a health score from regex or AST hits. A clean static result means only that no enabled rule detected a candidate in the reported coverage.

Record checks with command, target/features, environment and outcome. A successful host build does not prove another OS works. Screenshots do not prove keyboard or semantic behavior. When comparing revisions, preserve scope, data, viewport/window size and appearance.

Store reports under `.rupicola/visits/` only when the project context enables visit recording or the user asks. Use unique timestamp/scope filenames and never overwrite an earlier visit.

Label each finding's evidence as measured, seen (name the image), opinion or unverified (name what was missing), as in [visual verification](visual-verification.md) and [visual critique](critique.md).
