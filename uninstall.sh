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

echo "Uninstalling Octo from prefix: $PREFIX"

for file in "$BIN_DIR/octo" "$ICON_DIR/octo.svg" "$DESKTOP_DIR/octo.desktop"; do
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

echo "Octo uninstalled."
