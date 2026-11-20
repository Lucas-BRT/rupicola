#!/usr/bin/env bash
set -euo pipefail

REPO="${RUPICOLA_REPO:-Lucas-BRT/ferris-atelier}"
INSTALL_ROOT="${RUPICOLA_HOME:-$HOME/.local/share/rupicola/dist}"
BIN_DIR="${RUPICOLA_BIN_DIR:-$HOME/.local/bin}"
VERSION="${RUPICOLA_VERSION:-latest}"
FORCE_SOURCE=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      VERSION="${2:?missing value for --version}"
      shift 2
      ;;
    --source)
      FORCE_SOURCE=1
      shift
      ;;
    -h|--help)
      cat <<'EOF'
Rupicola installer

Usage:
  curl -fsSL https://rupicola.lucasbrt.dev/install.sh | bash
  curl -fsSL https://rupicola.lucasbrt.dev/install.sh | bash -s -- [--version v0.1.0] [--source]
EOF
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 2
      ;;
  esac
done

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}:${arch}" in
    Linux:x86_64|Linux:amd64) echo "x86_64-unknown-linux-gnu" ;;
    Darwin:arm64|Darwin:aarch64) echo "aarch64-apple-darwin" ;;
    Darwin:x86_64) echo "x86_64-apple-darwin" ;;
    *) echo "" ;;
  esac
}

install_from_source() {
  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found; install Rust from https://rustup.rs first." >&2
    exit 1
  fi
  echo "Installing rupicola from source via cargo..."
  cargo install --git "https://github.com/${REPO}.git" --locked
  echo "Installed: $(command -v rupicola || echo "$HOME/.cargo/bin/rupicola")"
}

verify_sha256() {
  local file="$1" sums="$2" expected actual
  expected="$(grep -F "  $(basename "$file")" "$sums" | awk '{print $1}' | head -n1)"
  if [ -z "$expected" ]; then
    echo "checksum entry not found for $(basename "$file")" >&2
    return 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$file" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  else
    echo "warning: sha256sum/shasum not found, skipping checksum verification" >&2
    return 0
  fi
  if [ "$actual" != "$expected" ]; then
    echo "checksum mismatch: expected $expected, got $actual" >&2
    return 1
  fi
}

if [ "$FORCE_SOURCE" -eq 1 ]; then
  install_from_source
  exit 0
fi

TARGET="$(detect_target)"
if [ -z "$TARGET" ]; then
  echo "Unsupported platform $(uname -s)/$(uname -m) for prebuilt binary; falling back to cargo." >&2
  install_from_source
  exit 0
fi

if [ "$VERSION" = "latest" ]; then
  TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1 || true)"
else
  TAG="$VERSION"
fi

if [ -z "$TAG" ]; then
  echo "No published release tag found yet; falling back to cargo install." >&2
  install_from_source
  exit 0
fi

ARCHIVE="rupicola-${TAG}-${TARGET}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading ${ARCHIVE}..."
if ! curl -fsSL "${BASE_URL}/${ARCHIVE}" -o "${TMP_DIR}/${ARCHIVE}"; then
  echo "Release archive not available yet; falling back to cargo install." >&2
  install_from_source
  exit 0
fi

if curl -fsSL "${BASE_URL}/SHA256SUMS" -o "${TMP_DIR}/SHA256SUMS"; then
  verify_sha256 "${TMP_DIR}/${ARCHIVE}" "${TMP_DIR}/SHA256SUMS"
fi

mkdir -p "$INSTALL_ROOT" "$BIN_DIR"
rm -rf "${INSTALL_ROOT:?}/rupicola"
tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "$INSTALL_ROOT"
ln -sf "${INSTALL_ROOT}/rupicola/rupicola" "${BIN_DIR}/rupicola"

for skill_dir in "$HOME/.agents/skills" "$HOME/.claude/skills"; do
  if [ -d "$(dirname "$skill_dir")" ] && [ ! -e "${skill_dir}/rupicola" ]; then
    mkdir -p "$skill_dir"
    ln -s "${INSTALL_ROOT}/rupicola" "${skill_dir}/rupicola"
    echo "Linked skill -> ${skill_dir}/rupicola"
  fi
done

echo "Installed rupicola (${TAG}) -> ${BIN_DIR}/rupicola"
if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
  echo "Add ${BIN_DIR} to your PATH: export PATH=\"${BIN_DIR}:\$PATH\""
fi
