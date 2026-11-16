# Rust UI integration

Inspect Cargo manifests, lockfile, features, target and actual runtime. Preserve existing state ownership. Do not introduce Tokio simply because code is Rust. Respect toolkit thread affinity and never hold UI context borrows across await. Schedule blocking work away from rendering; cancel obsolete tasks or identify results so stale responses cannot overwrite newer input. Preserve errors through the UI state model. Keep secrets and authorization on the trusted side of browser/IPC boundaries. Consult version-matching framework APIs before supplying code.
