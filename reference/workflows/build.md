# Build a Rust interface

1. Inspect the requested task, relevant project context, package manifest and resolved version/features. Preserve existing architecture unless a change is necessary and authorized.
2. Select the framework/rendering module and environment. Read UX and OS guidance only where it affects this slice.
3. Identify real inputs, state ownership, events, asynchronous boundaries and loading/error/retry behavior. Do not invent product data or introduce a runtime by habit.
4. Implement the smallest complete functional flow, reusing components and toolkit primitives. Keep tasks cancellable or reject stale results; preserve focus and entered data.
5. Build using the actual target/features and run meaningful checks. Exercise the flow and changed interaction where possible. Read reference/verification.md for evidence boundaries.
6. Report changes, observed checks and concrete remaining limits. Stop when the scoped flow works; unrelated enhancements stay outside the diff.

Resolve questions from the repository before asking. Existing user authorization applies to necessary in-scope work. New publishing, account mutations or expanded scope require their own authorization.
