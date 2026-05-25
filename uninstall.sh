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
MAN_DIR="$PREFIX/share/man/man1"

echo "Uninstalling Octa from prefix: $PREFIX"

for file in "$BIN_DIR/octa" "$ICON_DIR/octa.svg" "$DESKTOP_DIR/octa.desktop" "$MAN_DIR/octa.1" "$MAN_DIR/octa.1.gz"; do
	if [[ -f "$file" ]]; then
		rm -v "$file"
	else
		echo "Not found: $file (skipping)"
	fi
done

echo "Updating icon cache..."
if command -v gtk-update-icon-cache &>/dev/null; then
	gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi

if command -v update-desktop-database &>/dev/null; then
	update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

if command -v mandb &>/dev/null; then
	mandb --quiet "$MAN_DIR" 2>/dev/null || true
fi

echo "Octa uninstalled."
