# Static scan

Run `ferris-atelier discover <path> --json` to identify declared frameworks. Then run `ferris-atelier scan <path> --json`. The binary can be installed with Cargo; from the skill source checkout use `cargo run --locked -- scan <path> --json`.

For a standalone Rust file or deliberately selected framework, use `--framework <id>`. Do not select a framework merely to silence an unsupported result. `--include-tests` includes fixtures, tests, examples and benches otherwise excluded from traversal.

Read diagnostics, limitations and unsupported paths before findings. Confirm each candidate in project context; static rules do not establish accessibility or performance conformance. Stop after reporting candidates and coverage; do not edit application code.

Exit codes: 0 no candidates in available coverage, 1 error/diagnostic, 2 candidates, 3 no supported analysis or unsupported sources without candidates. Errors take precedence over findings; read JSON for the complete result.

Configure `.ferris-atelier/config.toml` with `ignore_rules = ["rule-id"]` and `ignore_paths = ["relative/path"]`. Paths are relative to the directory containing `.ferris-atelier`; unknown keys/rules and escaping paths are errors. A standalone `// ferris-ignore rule-id` comment immediately above a finding suppresses that line. Document why any exception is legitimate.
