# unslopovich-topcoat

A private fork of the `unslopovich` Claude Code skill that also reads
[Topcoat](https://github.com/tokio-rs/topcoat) sites (Rust, `view!` templates,
Tailwind).

## What differs from the original

- `scripts/rust-view.js`: keeps only `view!`, `attributes!` and `class!` bodies
  of a `.rs` file, blanks the rest (offsets and lines preserved), and
  `scripts/detect.js` scans that as web. A `Cargo.toml` that depends on
  `topcoat` counts as a web project.
- `reference/topcoat.md`: how to read Topcoat code and the web floor in
  Topcoat terms. Written from `reference/vendor/topcoat-llms-<tag>.txt`.
- `scripts/sync-topcoat.sh <tag>`: vendors Topcoat's `llms.txt` at a tag and
  records the tag and SHA-256 in `topcoat.lock`. The pinned tag must match the
  `topcoat` version of the site being reviewed.
- `tests/test.js`: `node tests/test.js`.

Everything else is the upstream skill, unchanged. It had no LICENSE file when
copied: keep this repository private until that is settled.

## Install

```sh
ln -s "$PWD" ~/.claude/skills/unslopovich-topcoat
```

Rename it to `unslopovich` (after moving the original away) to replace it.

## Updating for a new Topcoat

```sh
scripts/sync-topcoat.sh v0.12.1
diff reference/vendor/topcoat-llms-v0.11.0.txt reference/vendor/topcoat-llms-v0.12.1.txt
```

Edit `reference/topcoat.md` for what changed, delete the old vendored file.
