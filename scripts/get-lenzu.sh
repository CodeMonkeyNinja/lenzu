#!/usr/bin/env bash
# Download the latest Lenzu AppImage from GitHub releases and install it.
#
# Usage:
#   ./scripts/get-lenzu.sh                          # latest stable release
#   ./scripts/get-lenzu.sh v0.4.0                    # specific version
#   ./scripts/get-lenzu.sh --bundle                  # also download the full bundle
#
# Installs to ~/.local/share/lenzu/ with a symlink at ~/.local/bin/lenzu.

set -euo pipefail

REPO="CodeMonkeyNinja/lenzu"
ARCH="x86_64"
INSTALL_DIR="${HOME}/.local/share/lenzu"
BIN_DIR="${HOME}/.local/bin"
BUNDLE=false

for arg in "$@"; do
  case "$arg" in
    --bundle) BUNDLE=true ;;
    -h|--help)
      sed -n '3,9p' "$0"
      exit 0
      ;;
    v*) TAG="$arg" ;;
  esac
done

# Resolve the latest release tag if none given
if [[ -z "${TAG:-}" ]]; then
  echo "[get-lenzu] Fetching latest release tag..."
  if command -v gh &>/dev/null; then
    TAG="$(gh release list --limit 1 --json tagName --jq '.[0].tagName')"
  else
    TAG="$(curl -sSfL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": "\(.*\)",.*/\1/')"
  fi
fi

VERSION="${TAG#v}"
echo "[get-lenzu] Release: ${TAG}"

# Ensure install directories exist
mkdir -p "$INSTALL_DIR" "$BIN_DIR"

# Download to a .part file and rename only after curl succeeds, so an
# interrupted transfer never leaves a truncated file that a later run mistakes
# for a complete download. curl -C - resumes an existing .part rather than
# restarting from byte 0. (ref #64)
fetch() {
    local url="$1" dest="$2"
    local part="${dest}.part"
    if [[ -f "$dest" ]]; then
        echo "[get-lenzu] $(basename "$dest") already downloaded"
        return 0
    fi
    echo "[get-lenzu] Downloading $(basename "$dest")..."
    if ! curl -sSfL -C - "$url" -o "$part"; then
        rm -f "$part"
        echo "[get-lenzu] ERROR: download failed: $url" >&2
        return 1
    fi
    mv -f "$part" "$dest"
}

# Download the AppImage
APPIMAGE="lenzu-${VERSION}-${ARCH}.appimage"
APPIMAGE_URL="https://github.com/${REPO}/releases/download/${TAG}/${APPIMAGE}"

fetch "$APPIMAGE_URL" "${INSTALL_DIR}/${APPIMAGE}"
chmod +x "${INSTALL_DIR}/${APPIMAGE}"

# Symlink to ~/.local/bin
SYMLINK="${BIN_DIR}/lenzu"
if [[ -L "$SYMLINK" ]] && [[ "$(readlink "$SYMLINK")" == "${INSTALL_DIR}/${APPIMAGE}" ]]; then
  echo "[get-lenzu] Symlink ${SYMLINK} already up-to-date"
else
  ln -sf "${INSTALL_DIR}/${APPIMAGE}" "$SYMLINK"
  echo "[get-lenzu] Symlinked ${SYMLINK} → ${APPIMAGE}"
fi

# Optionally download the full bundle
if [[ "$BUNDLE" == "true" ]]; then
  BUNDLE_FILE="lenzu-bundle-${VERSION}.tar"
  BUNDLE_URL="https://github.com/${REPO}/releases/download/${TAG}/${BUNDLE_FILE}"

  fetch "$BUNDLE_URL" "${INSTALL_DIR}/${BUNDLE_FILE}"
  echo "[get-lenzu] Extract with: tar -xf ${INSTALL_DIR}/${BUNDLE_FILE} -C ${INSTALL_DIR}"
fi

# Remind about PATH
case ":${PATH}:" in
  *:"${BIN_DIR}":*) ;;
  *) echo "[get-lenzu] NOTE: ${BIN_DIR} is not in PATH. Add it:"
     echo "  export PATH=\"\${PATH}:${BIN_DIR}\"" ;;
esac

echo "[get-lenzu] Done. Run: lenzu"
