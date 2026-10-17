#!/usr/bin/env bash
set -euo pipefail

# get-octa.sh - download the latest Octa release and install it.
#
# Linux: fetches the newest release tarball from GitHub, extracts it, and
# hands off to the bundled install.sh (which installs the binary, icon,
# desktop entry, man page, and licences). An optional first argument is the
# install prefix and is passed straight through to install.sh:
#
#   curl -fsSL .../get-octa.sh | sudo bash            # system-wide (/usr/local)
#   curl -fsSL .../get-octa.sh | bash -s -- ~/.local  # user-local (no sudo)
#
# macOS (Apple Silicon): puts Octa.app into ~/Applications, or the folder
# given as the first argument, and links the command line tool into
# ~/.local/bin. No sudo. A file curl downloads carries no quarantine
# attribute, so Gatekeeper does not stop the first launch:
#
#   curl -fsSL .../get-octa.sh | bash
#   curl -fsSL .../get-octa.sh | bash -s -- /Applications  # admin accounts
#
# Both verify the download against the release's SHA256SUMS.

REPO="thorstenfoltz/octa"

fetch() { # fetch URL [OUTFILE]
	if command -v curl &>/dev/null; then
		if [[ $# -eq 2 ]]; then curl -fL --progress-bar "$1" -o "$2"; else curl -fsSL "$1"; fi
	elif command -v wget &>/dev/null; then
		if [[ $# -eq 2 ]]; then wget -q "$1" -O "$2"; else wget -qO- "$1"; fi
	else
		echo "Error: curl or wget is required to download Octa."
		exit 1
	fi
}

case "$(uname -s)-$(uname -m)" in
Linux-x86_64) PLATFORM="linux-x86_64" ;;
Darwin-arm64) PLATFORM="macos-aarch64" ;;
Darwin-*)
	echo "Error: releases ship for Apple Silicon Macs only."
	echo "On an Intel Mac, build from source: https://thorstenfoltz.github.io/octa/getting-started/installation/"
	exit 1
	;;
*)
	echo "Error: no release build for $(uname -s) $(uname -m)."
	echo "Build from source: https://thorstenfoltz.github.io/octa/getting-started/installation/"
	exit 1
	;;
esac

API_RESPONSE=$(fetch "https://api.github.com/repos/${REPO}/releases/latest")
VERSION=$(printf '%s' "$API_RESPONSE" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [[ -z "$VERSION" ]]; then
	echo "Error: Could not determine the latest release version."
	echo "Check your internet connection or visit https://github.com/${REPO}/releases."
	exit 1
fi

NAME="octa-${VERSION}-${PLATFORM}"
TARBALL="${NAME}.tar.gz"
BASE="https://github.com/${REPO}/releases/download/${VERSION}"
DOWNLOAD_TMP="$(mktemp -d)"
trap 'rm -rf "$DOWNLOAD_TMP"' EXIT

echo "Downloading Octa ${VERSION}..."
fetch "$BASE/$TARBALL" "$DOWNLOAD_TMP/$TARBALL"
fetch "$BASE/SHA256SUMS" "$DOWNLOAD_TMP/SHA256SUMS"

echo "Verifying..."
EXPECTED=$(grep " \*\?${TARBALL}\$" "$DOWNLOAD_TMP/SHA256SUMS" | cut -d' ' -f1)
if command -v sha256sum &>/dev/null; then
	ACTUAL=$(sha256sum "$DOWNLOAD_TMP/$TARBALL" | cut -d' ' -f1)
else
	ACTUAL=$(shasum -a 256 "$DOWNLOAD_TMP/$TARBALL" | cut -d' ' -f1)
fi
if [[ -z "$EXPECTED" || "$EXPECTED" != "$ACTUAL" ]]; then
	echo "Error: the download does not match the release's SHA256SUMS. Nothing was installed."
	exit 1
fi

echo "Extracting..."
tar -xzf "$DOWNLOAD_TMP/$TARBALL" -C "$DOWNLOAD_TMP"
EXTRACT_DIR="$DOWNLOAD_TMP/$NAME"

if [[ "$PLATFORM" == linux-* ]]; then
	if [[ ! -f "$EXTRACT_DIR/install.sh" ]]; then
		echo "Error: install.sh not found in the downloaded release package."
		exit 1
	fi
	echo "Running the bundled installer..."
	chmod +x "$EXTRACT_DIR/install.sh"
	"$EXTRACT_DIR/install.sh" "$@"
	exit 0
fi

APPS="${1:-$HOME/Applications}"
mkdir -p "$APPS"
rm -rf "$APPS/Octa.app"
# ditto keeps the bundle's signature and attributes intact.
ditto "$EXTRACT_DIR/Octa.app" "$APPS/Octa.app"
echo "Installed $APPS/Octa.app"

BIN="$HOME/.local/bin"
mkdir -p "$BIN"
ln -sf "$APPS/Octa.app/Contents/MacOS/octa" "$BIN/octa"
echo "Linked the command line tool to $BIN/octa"
case ":$PATH:" in
*":$BIN:"*) ;;
*) echo "Add it to your PATH: echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zprofile" ;;
esac
