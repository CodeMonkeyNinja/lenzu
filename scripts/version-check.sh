#!/usr/bin/env bash
# Verify that VERSION, Cargo.toml, and package.json are all in sync.
# Run as part of CI or pre-release.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAILED=0

canonical=$(cat "$REPO_ROOT/VERSION" | tr -d '[:space:]')

echo "=== Version sync check ==="
echo "  VERSION:       $canonical"

# Cargo.toml (Rust client)
cargo_version=$(grep '^version' "$REPO_ROOT/lenzu/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/' | tr -d '[:space:]')
echo "  lenzu/Cargo.toml:  $cargo_version"
if [[ "$cargo_version" != "$canonical" ]]; then
  echo "  MISMATCH!"
  FAILED=1
fi

# package.json (Electron HUD server)
pkg_version=$(grep '"version"' "$REPO_ROOT/lenzu_server/package.json" | head -1 | sed 's/.*"version": "\(.*\)".*/\1/' | tr -d '[:space:]')
echo "  lenzu_server/package.json: $pkg_version"
if [[ "$pkg_version" != "$canonical" ]]; then
  echo "  MISMATCH!"
  FAILED=1
fi

echo ""
if [[ "$FAILED" -eq 1 ]]; then
  echo "FAILED — versions are not in sync. Update VERSION, then sync Cargo.toml and package.json."
  exit 1
fi

echo "OK — all versions match."
