#!/usr/bin/env bash
set -euo pipefail

# Build a local Arch package from the current working tree for testing.
#
# Usage: ./pkg/build-local.sh [source|binary] [version]
#   source  — compiles from source during install (mirrors AUR experience)
#   binary  — pre-compiled, fast install
#   version defaults to 0.0.1
#
# Install: paru -U pkg/build/<type>/octo-*.pkg.tar.zst
# Remove:  paru -R octo  or  paru -R octo-bin

TYPE="${1:-}"
VERSION="${2:-0.0.1}"

if [[ "$TYPE" != "source" && "$TYPE" != "binary" ]]; then
	echo "Usage: $0 <source|binary> [version]"
	echo ""
	echo "  source  — compiles from source during install (mirrors AUR)"
	echo "  binary  — pre-compiled binary, fast install"
	exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$SCRIPT_DIR/build/$TYPE"
ARCH="$(uname -m)"

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

if [[ "$TYPE" == "source" ]]; then
	# -------------------------------------------------------------------
	# Source package — compiles during install
	# -------------------------------------------------------------------
	echo "==> Creating source tarball for version $VERSION..."
	tar czf "$BUILD_DIR/octo-$VERSION.tar.gz" \
		--transform "s,^,octo-$VERSION/," \
		--exclude='.git' \
		--exclude='target' \
		--exclude='pkg/build' \
		-C "$REPO_DIR" .

	cat >"$BUILD_DIR/PKGBUILD" <<'PKGBUILD_EOF'
# Local test build (source) — not for AUR
pkgname=octo
pkgver=VERSION_PLACEHOLDER
pkgrel=1
pkgdesc="A modular multi-format data viewer and editor"
arch=('x86_64')
url="https://github.com/thorstenfoltz/octo"
license=('MIT')
depends=('gtk3' 'libxcb' 'libxkbcommon' 'openssl' 'fontconfig' 'freetype2' 'harfbuzz' 'fribidi' 'libjpeg-turbo' 'openjpeg2' 'gumbo-parser' 'jbig2dec' 'mujs')
makedepends=('rust' 'cargo' 'clang' 'cmake' 'nasm' 'pkgconf')
conflicts=('octo-bin')
options=(!lto)
source=("octo-$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
    cd "$pkgname-$pkgver"
    sed -i "s/^version = .*/version = \"$pkgver\"/" Cargo.toml
    export RUSTUP_TOOLCHAIN=stable
    cargo update --workspace
    cargo fetch --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$pkgname-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    export CARGO_BUILD_JOBS="$(nproc)"
    export MAKEFLAGS="-j$(nproc)"
    cargo build --frozen --release
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/octo" "$pkgdir/usr/bin/octo"
    install -Dm644 "assets/octo.svg" "$pkgdir/usr/share/icons/hicolor/scalable/apps/octo.svg"
    install -Dm644 "octo.desktop" "$pkgdir/usr/share/applications/octo.desktop"
    install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
PKGBUILD_EOF

	sed -i "s/VERSION_PLACEHOLDER/$VERSION/" "$BUILD_DIR/PKGBUILD"

	echo ""
	echo "Done! Source package prepared. Build and install with:"
	echo "  cd $BUILD_DIR && paru -Ui"

else
	# -------------------------------------------------------------------
	# Binary package — pre-compiled, just installs files
	# -------------------------------------------------------------------
	echo "==> Building release binary..."
	cd "$REPO_DIR"
	sed -i "s/^version = .*/version = \"$VERSION\"/" Cargo.toml
	cargo build --release 2>&1
	sed -i 's/^version = .*/version = "0.0.0-dev"/' Cargo.toml

	cp "$REPO_DIR/target/release/octo" "$BUILD_DIR/octo"
	cp "$REPO_DIR/assets/octo.svg" "$BUILD_DIR/octo.svg"
	cp "$REPO_DIR/octo.desktop" "$BUILD_DIR/octo.desktop"
	cp "$REPO_DIR/LICENSE" "$BUILD_DIR/LICENSE"

	cat >"$BUILD_DIR/PKGBUILD" <<'PKGBUILD_EOF'
# Local test build (binary) — not for AUR
pkgname=octo-bin
pkgver=VERSION_PLACEHOLDER
pkgrel=1
pkgdesc="A modular multi-format data viewer and editor (pre-compiled)"
arch=('x86_64')
url="https://github.com/thorstenfoltz/octo"
license=('MIT')
depends=('gtk3' 'libxcb' 'libxkbcommon' 'openssl' 'fontconfig' 'freetype2' 'harfbuzz' 'fribidi' 'libjpeg-turbo' 'openjpeg2' 'gumbo-parser' 'jbig2dec' 'mujs')
provides=('octo')
conflicts=('octo')
source=('octo' 'octo.svg' 'octo.desktop' 'LICENSE')
sha256sums=('SKIP' 'SKIP' 'SKIP' 'SKIP')

package() {
    install -Dm755 "$srcdir/octo" "$pkgdir/usr/bin/octo"
    install -Dm644 "$srcdir/octo.svg" "$pkgdir/usr/share/icons/hicolor/scalable/apps/octo.svg"
    install -Dm644 "$srcdir/octo.desktop" "$pkgdir/usr/share/applications/octo.desktop"
    install -Dm644 "$srcdir/LICENSE" "$pkgdir/usr/share/licenses/octo/LICENSE"
}
PKGBUILD_EOF

	sed -i "s/VERSION_PLACEHOLDER/$VERSION/" "$BUILD_DIR/PKGBUILD"

	echo "==> Packaging binary..."
	cd "$BUILD_DIR"
	makepkg -sf

	PKG="$BUILD_DIR/octo-bin-${VERSION}-1-${ARCH}.pkg.tar.zst"
	echo ""
	echo "Done! Install with:"
	echo "  paru -U $PKG"
fi

PKGNAME="octo"
[[ "$TYPE" == "binary" ]] && PKGNAME="octo-bin"
echo ""
echo "Remove with:  paru -R $PKGNAME"
