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

# Download the AppImage
APPIMAGE="lenzu-${VERSION}-${ARCH}.appimage"
APPIMAGE_URL="https://github.com/${REPO}/releases/download/${TAG}/${APPIMAGE}"

if [[ -f "${INSTALL_DIR}/${APPIMAGE}" ]]; then
  echo "[get-lenzu] ${APPIMAGE} already downloaded"
else
  echo "[get-lenzu] Downloading ${APPIMAGE}..."
  curl -sSfL "$APPIMAGE_URL" -o "${INSTALL_DIR}/${APPIMAGE}"
  chmod +x "${INSTALL_DIR}/${APPIMAGE}"
fi

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

  if [[ -f "${INSTALL_DIR}/${BUNDLE_FILE}" ]]; then
    echo "[get-lenzu] ${BUNDLE_FILE} already downloaded"
  else
    echo "[get-lenzu] Downloading ${BUNDLE_FILE} (large)..."
    curl -sSfL "$BUNDLE_URL" -o "${INSTALL_DIR}/${BUNDLE_FILE}"
    echo "[get-lenzu] Extract with: tar -xf ${INSTALL_DIR}/${BUNDLE_FILE} -C ${INSTALL_DIR}"
  fi
fi

# Remind about PATH
case ":${PATH}:" in
  *:"${BIN_DIR}":*) ;;
  *) echo "[get-lenzu] NOTE: ${BIN_DIR} is not in PATH. Add it:"
     echo "  export PATH=\"\${PATH}:${BIN_DIR}\"" ;;
esac

echo "[get-lenzu] Done. Run: lenzu"
