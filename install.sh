#!/usr/bin/env bash
set -euo pipefail

# Detect default prefix: Arch Linux uses /usr, others use /usr/local
if [ -z "${1:-}" ]; then
	if [ -f /etc/arch-release ]; then
		PREFIX="/usr"
	else
		PREFIX="/usr/local"
	fi
else
	PREFIX="$1"
fi
BIN_DIR="$PREFIX/bin"
ICON_DIR="$PREFIX/share/icons/hicolor/scalable/apps"
DESKTOP_DIR="$PREFIX/share/applications"
DOC_DIR="$PREFIX/share/doc/octa"
MAN_DIR="$PREFIX/share/man/man1"

# SCRIPT_DIR: directory containing this script; empty when piped via curl | bash
SELF="${BASH_SOURCE[0]:-}"
if [[ -n "$SELF" && -f "$SELF" ]]; then
	SCRIPT_DIR="$(cd "$(dirname "$SELF")" && pwd)"
else
	SCRIPT_DIR=""
fi

# ASSET_DIR: where to find support files (icon, desktop entry, man page, licences).
# Defaults to SCRIPT_DIR; overridden below when we download a release.
ASSET_DIR="${SCRIPT_DIR}"

BINARY=""

# 1. Pre-built binary next to this script (release tarball use case)
if [[ -n "$SCRIPT_DIR" && -f "$SCRIPT_DIR/octa" ]]; then
	BINARY="$SCRIPT_DIR/octa"
	echo "Using pre-built binary."
# 2. Binary already compiled in a local source checkout
elif [[ -n "$SCRIPT_DIR" && -f "$SCRIPT_DIR/target/release/octa" ]]; then
	BINARY="$SCRIPT_DIR/target/release/octa"
	echo "Using previously built binary at target/release/octa."
# 3. Download the latest release from GitHub
else
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

	ASSET_DIR="$DOWNLOAD_TMP/octa-${VERSION}-linux-x86_64"
	BINARY="$ASSET_DIR/octa"

	if [[ ! -f "$BINARY" ]]; then
		echo "Error: Binary not found in the downloaded release package."
		exit 1
	fi
	echo "Octa ${VERSION} downloaded."
fi

echo "Installing binary to $BIN_DIR..."
install -Dm755 "$BINARY" "$BIN_DIR/octa"

echo "Installing icon to $ICON_DIR..."
install -Dm644 "$ASSET_DIR/assets/octa.svg" "$ICON_DIR/octa.svg"

echo "Installing desktop entry to $DESKTOP_DIR..."
install -Dm644 "$ASSET_DIR/octa.desktop" "$DESKTOP_DIR/octa.desktop"

# Man page. Release tarballs ship the pre-rendered octa.1; source checkouts can
# render it on the fly if asciidoctor is on PATH.
MAN_SRC=""
if [[ -f "$ASSET_DIR/octa.1" ]]; then
	MAN_SRC="$ASSET_DIR/octa.1"
elif [[ -n "$SCRIPT_DIR" && -f "$SCRIPT_DIR/docs/cli/octa.1.adoc" ]] && command -v asciidoctor >/dev/null; then
	echo "Rendering man page from docs/cli/octa.1.adoc..."
	asciidoctor -b manpage "$SCRIPT_DIR/docs/cli/octa.1.adoc" -o "$SCRIPT_DIR/octa.1"
	MAN_SRC="$SCRIPT_DIR/octa.1"
fi
if [[ -n "$MAN_SRC" ]]; then
	echo "Installing man page to $MAN_DIR..."
	install -Dm644 "$MAN_SRC" "$MAN_DIR/octa.1"
	if command -v mandb >/dev/null; then
		mandb --quiet "$MAN_DIR" 2>/dev/null || true
	fi
else
	echo "No man page available (no octa.1 next to script and asciidoctor not found)."
	echo "  Install \`asciidoctor\` and rerun if you want \`man octa\` to work."
fi

if [[ -f "$ASSET_DIR/THIRD_PARTY_LICENSES.md" ]]; then
	echo "Installing third-party licence bundle to $DOC_DIR..."
	install -Dm644 "$ASSET_DIR/THIRD_PARTY_LICENSES.md" "$DOC_DIR/THIRD_PARTY_LICENSES.md"
fi
if [[ -f "$ASSET_DIR/LICENSE" ]]; then
	install -Dm644 "$ASSET_DIR/LICENSE" "$DOC_DIR/LICENSE"
fi
if [[ -d "$ASSET_DIR/licenses" ]]; then
	for f in "$ASSET_DIR/licenses"/*.txt; do
		[[ -f "$f" ]] || continue
		install -Dm644 "$f" "$DOC_DIR/licenses/$(basename "$f")"
	done
fi

echo "Updating icon cache..."
if command -v gtk-update-icon-cache &>/dev/null; then
	gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi

if command -v update-desktop-database &>/dev/null; then
	update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

echo "Octa installed successfully."
echo "  Binary:  $BIN_DIR/octa"
echo "  Icon:    $ICON_DIR/octa.svg"
echo "  Desktop: $DESKTOP_DIR/octa.desktop"
if [[ -f "$MAN_DIR/octa.1" ]]; then
	echo "  Man:     $MAN_DIR/octa.1   (try \`man octa\`)"
fi
