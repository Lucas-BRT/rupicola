#!/usr/bin/env bash
# Vendors Topcoat's llms.txt at a pinned tag: scripts/sync-topcoat.sh v0.11.0
# Writes reference/vendor/topcoat-llms-<tag>.txt and records tag + SHA-256 in
# topcoat.lock. Re-run with a new tag, then update reference/topcoat.md by hand
# from `diff` of the two vendored files.
set -euo pipefail
tag="${1:?usage: sync-topcoat.sh <tag, e.g. v0.11.0>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/reference/vendor/topcoat-llms-$tag.txt"
mkdir -p "$root/reference/vendor"
curl -fsSL "https://raw.githubusercontent.com/tokio-rs/topcoat/$tag/llms.txt" -o "$out"
sum="$(sha256sum "$out" | cut -d' ' -f1)"
printf 'tag=%s\nsha256=%s\n' "$tag" "$sum" > "$root/topcoat.lock"
echo "$tag $sum"
