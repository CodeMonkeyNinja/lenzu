#!/usr/bin/env bash
# Run all unit tests for both the Rust client and the Electron HUD server.
#
# Usage:
#   scripts/unit-test.sh            # both cargo test + vitest
#   scripts/unit-test.sh --rust     # Rust only
#   scripts/unit-test.sh --ts       # TypeScript only

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

run_rust() {
  echo "==> cargo test (Rust client)..."
  cd "$REPO_ROOT"
  cargo test 2>&1
  echo ""
}

run_ts() {
  echo "==> vitest (lenzu_server)..."
  cd "$REPO_ROOT/lenzu_server"
  npx vitest run 2>&1
  echo ""
}

case "${1:-}" in
  --rust) run_rust ;;
  --ts)   run_ts ;;
  *)
    failed=0
    run_rust || failed=1
    run_ts   || failed=1
    exit "$failed"
    ;;
esac
