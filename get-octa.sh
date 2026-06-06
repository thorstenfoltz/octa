#!/usr/bin/env bash
set -euo pipefail

# get-octa.sh - download the latest Octa release and install it.
#
# Fetches the newest release tarball from GitHub, extracts it, and hands off to
# the bundled install.sh (which installs the binary, icon, desktop entry, man
# page, and licences). An optional first argument is the install prefix and is
# passed straight through to install.sh:
#
#   curl -fsSL .../get-octa.sh | sudo bash            # system-wide (/usr/local)
#   curl -fsSL .../get-octa.sh | bash -s -- ~/.local  # user-local (no sudo)

REPO="thorstenfoltz/octa"

if command -v curl &>/dev/null; then
	API_RESPONSE=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest")
elif command -v wget &>/dev/null; then
	API_RESPONSE=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest")
else
	echo "Error: curl or wget is required to download Octa."
	exit 1
fi

VERSION=$(printf '%s' "$API_RESPONSE" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [[ -z "$VERSION" ]]; then
	echo "Error: Could not determine the latest release version."
	echo "Check your internet connection or visit https://github.com/${REPO}/releases."
	exit 1
fi

TARBALL="octa-${VERSION}-linux-x86_64.tar.gz"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${TARBALL}"
DOWNLOAD_TMP="$(mktemp -d)"
trap 'rm -rf "$DOWNLOAD_TMP"' EXIT

echo "Downloading Octa ${VERSION}..."
if command -v curl &>/dev/null; then
	curl -fL --progress-bar "$URL" -o "$DOWNLOAD_TMP/$TARBALL"
else
	wget -q "$URL" -O "$DOWNLOAD_TMP/$TARBALL"
fi

echo "Extracting..."
tar -xzf "$DOWNLOAD_TMP/$TARBALL" -C "$DOWNLOAD_TMP"

EXTRACT_DIR="$DOWNLOAD_TMP/octa-${VERSION}-linux-x86_64"
if [[ ! -f "$EXTRACT_DIR/install.sh" ]]; then
	echo "Error: install.sh not found in the downloaded release package."
	exit 1
fi

echo "Running the bundled installer..."
chmod +x "$EXTRACT_DIR/install.sh"
"$EXTRACT_DIR/install.sh" "$@"
