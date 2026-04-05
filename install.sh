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

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# If a pre-built binary exists next to this script, use it; otherwise build from source
if [[ -f "$SCRIPT_DIR/octo" ]]; then
	BINARY="$SCRIPT_DIR/octo"
	echo "Using pre-built binary."
elif command -v cargo &>/dev/null; then
	echo "Building Octo (release)..."
	cargo build --release
	BINARY="$SCRIPT_DIR/target/release/octo"
else
	echo "Error: No pre-built binary found and cargo is not installed."
	echo "Install Rust from https://rustup.rs/ or download a pre-built release."
	exit 1
fi

echo "Installing binary to $BIN_DIR..."
install -Dm755 "$BINARY" "$BIN_DIR/octo"

echo "Installing icon to $ICON_DIR..."
install -Dm644 "$SCRIPT_DIR/assets/octo.svg" "$ICON_DIR/octo.svg"

echo "Installing desktop entry to $DESKTOP_DIR..."
install -Dm644 "$SCRIPT_DIR/octo.desktop" "$DESKTOP_DIR/octo.desktop"

echo "Updating icon cache..."
if command -v gtk-update-icon-cache &>/dev/null; then
	gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi

if command -v update-desktop-database &>/dev/null; then
	update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

echo "Octo installed successfully."
echo "  Binary:  $BIN_DIR/octo"
echo "  Icon:    $ICON_DIR/octo.svg"
echo "  Desktop: $DESKTOP_DIR/octo.desktop"
