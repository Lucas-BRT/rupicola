# Verification and performance

Choose checks that establish the changed behavior. Build the actual package/features/target, run relevant tests, then exercise the changed task, keyboard/focus, error recovery, resizing and text scaling where execution is available. Track compilation separately from interaction and assistive-technology results.

For performance questions, measure a representative workload. Record hardware, OS, renderer, build profile, dataset, repetition and measured latency or frame timing. Compare equivalent conditions. UI-thread blocking, expensive render functions and eager large lists are investigation leads; source inspection alone cannot establish healthy performance.

For web, inspect transferred assets/WASM initialization, server latency, hydration and browser responsiveness as applicable. For desktop, inspect event-loop stalls, text/layout work, frame scheduling and background tasks. Respect reduced motion and distinguish input acknowledgement from completion of an operation.

If a check cannot run, identify the exact missing dependency, runtime, device or permission and its impact on confidence. Do not mark unavailable checks as passed.
