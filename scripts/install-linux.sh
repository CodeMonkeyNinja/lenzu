#!/usr/bin/env bash
# One-shot installer for Lenzu on Linux (Debian/Ubuntu derivatives).
# Installs system packages (MeCab, fonts) then downloads and symlinks
# the latest Lenzu AppImage to ~/.local/bin/lenzu.
#
# Usage:
#   curl -sSfL https://raw.githubusercontent.com/CodeMonkeyNinja/lenzu/trunk/scripts/install-linux.sh | bash
#
# Flags (pass via environment or modify the curl pipe):
#   SKIP_APT=1    skip apt install (already have deps)
#   LENZU_TAG=    pin a specific release tag (e.g. v1.0.0)

set -euo pipefail

REPO="CodeMonkeyNinja/lenzu"
BRANCH="trunk"

# ── System dependencies ──────────────────────────────────────────────────────────
if [[ "${SKIP_APT:-}" != "1" ]]; then
    echo "[install-linux] Installing system dependencies..."
    sudo apt update
    sudo apt install -y mecab mecab-ipadic-utf8 fonts-noto-cjk
else
    echo "[install-linux] SKIP_APT=1 — skipping apt install"
fi

# ── Download and install the AppImage ─────────────────────────────────────────────
GET_LENZU_URL="https://raw.githubusercontent.com/${REPO}/${BRANCH}/scripts/get-lenzu.sh"

if [[ -n "${LENZU_TAG:-}" ]]; then
    echo "[install-linux] Installing Lenzu ${LENZU_TAG}..."
    curl -sSfL "$GET_LENZU_URL" | bash -s -- "${LENZU_TAG}"
else
    echo "[install-linux] Installing latest Lenzu..."
    curl -sSfL "$GET_LENZU_URL" | bash
fi

echo ""
echo "[install-linux] Done."
echo "    Run:   lenzu"
echo "    Help:  lenzu --help"
