#!/usr/bin/env bash
# Build a Microsoft Store MSIX from a published Octa release, on Linux.
#
#   ./windows/build-msix.sh            # newest release
#   ./windows/build-msix.sh 0.15.1     # a specific tag
#
# The package lands next to this script as windows/octa-<version>.0.msix and is
# gitignored. Upload it to Partner Center under "Pakete".
#
# Why this exists: the release workflow packs the MSIX with makeappx.exe, which
# only runs on Windows, and a Release dispatch is a heavyweight way to obtain
# one package. This does the same steps against an already-published release.
#
# It produces an UNSIGNED package, which is correct: Partner Center accepts
# unsigned packages and the Store signs them for distribution.
#
# First run builds Microsoft's cross-platform packer (msix-packaging) into
# windows/.msix-tools/ and takes a few minutes. Later runs reuse it.

set -euo pipefail

REPO_SLUG="thorstenfoltz/octa"
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$HERE/.." && pwd)"
TOOLS="$HERE/.msix-tools"
MAKEMSIX="$TOOLS/bin/makemsix"

for cmd in curl unzip magick cmake make git; do
	command -v "$cmd" >/dev/null || {
		echo "error: '$cmd' is required." >&2
		exit 1
	}
done

# --- resolve the version -----------------------------------------------------
VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
	echo "Resolving newest release..."
	# Captured rather than piped straight into grep: `grep -m1` closes the pipe
	# on the first match and curl then dies with "failure writing output".
	latest_json="$(curl -fsSL "https://api.github.com/repos/$REPO_SLUG/releases/latest")"
	VERSION="$(printf '%s' "$latest_json" | grep -m1 '"tag_name"' | sed 's/.*: *"//; s/".*//')"
	[[ -n "$VERSION" ]] || {
		echo "error: could not read the latest tag." >&2
		exit 1
	}
fi
echo "Version: $VERSION"

# --- build the packer once ---------------------------------------------------
if [[ ! -x "$MAKEMSIX" ]]; then
	echo "Building makemsix (first run only, a few minutes)..."
	BUILD_TMP="$(mktemp -d)"
	trap 'rm -rf "$BUILD_TMP"' EXIT
	git clone --depth 1 -q https://github.com/microsoft/msix-packaging.git "$BUILD_TMP/src"
	# The bundled build pins C++14, but the system ICU headers this pulls in
	# need C++17 (std::basic_string_view), so the stock build fails outright.
	sed -i 's/set(CMAKE_CXX_STANDARD 14)/set(CMAKE_CXX_STANDARD 17)/' \
		"$BUILD_TMP/src/CMakeLists.txt" "$BUILD_TMP/src/lib/xerces/CMakeLists.txt"
	mkdir -p "$BUILD_TMP/src/.vs"
	(
		cd "$BUILD_TMP/src/.vs"
		# MSIX_PACK=on is NOT the default; without it makemsix can only unpack.
		cmake -DCMAKE_BUILD_TYPE=MinSizeRel -DSKIP_BUNDLES=off \
			-DUSE_VALIDATION_PARSER=on -DMSIX_PACK=on \
			-DMSIX_SAMPLES=off -DMSIX_TESTS=off -DLINUX=on \
			-DCMAKE_TOOLCHAIN_FILE=../cmake/linux.cmake .. >cmake.log 2>&1
		make -j"$(nproc)" >make.log 2>&1
	) || {
		echo "error: makemsix build failed; see $BUILD_TMP/src/.vs/make.log" >&2
		trap - EXIT
		exit 1
	}
	mkdir -p "$TOOLS/bin" "$TOOLS/lib"
	cp "$BUILD_TMP/src/.vs/bin/makemsix" "$TOOLS/bin/"
	cp "$BUILD_TMP/src/.vs/lib/libmsix.so" "$TOOLS/lib/"
	rm -rf "$BUILD_TMP"
	trap - EXIT
	echo "  cached in $TOOLS"
fi
export LD_LIBRARY_PATH="$TOOLS/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# --- fetch and verify the released binary ------------------------------------
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
ZIP="octa-$VERSION-windows-x86_64.zip"
BASE="https://github.com/$REPO_SLUG/releases/download/$VERSION"

echo "Downloading $ZIP..."
curl -fsSL -o "$WORK/$ZIP" "$BASE/$ZIP"
curl -fsSL -o "$WORK/SHA256SUMS" "$BASE/SHA256SUMS"
(cd "$WORK" && grep -- "$ZIP" SHA256SUMS | sha256sum -c -) ||
	{
		echo "error: checksum mismatch, refusing to package." >&2
		exit 1
	}

unzip -q -o "$WORK/$ZIP" -d "$WORK/extracted"
[[ -f "$WORK/extracted/octa.exe" ]] || {
	echo "error: octa.exe not in the zip." >&2
	exit 1
}

# --- stage the payload (mirrors the release workflow) ------------------------
mkdir -p "$WORK/msix/Images"
cp "$WORK/extracted/octa.exe" "$WORK/msix/octa.exe"
magick "$ROOT/assets/octa.png" -resize 44x44 "$WORK/msix/Images/Square44x44Logo.png"
magick "$ROOT/assets/octa.png" -resize 71x71 "$WORK/msix/Images/Square71x71Logo.png"
magick "$ROOT/assets/octa.png" -resize 150x150 "$WORK/msix/Images/Square150x150Logo.png"
magick "$ROOT/assets/octa.png" -resize 310x310 "$WORK/msix/Images/Square310x310Logo.png"
magick "$ROOT/assets/octa.png" -resize 50x50 "$WORK/msix/Images/StoreLogo.png"
magick "$ROOT/assets/octa.png" -resize 150x150 -background none -gravity center \
	-extent 310x150 "$WORK/msix/Images/Wide310x150Logo.png"

# The Store requires a four-part version whose last part is 0.
sed "s/Version=\"0\.0\.0\.0\"/Version=\"$VERSION.0\"/" \
	"$HERE/AppxManifest.xml" >"$WORK/msix/AppxManifest.xml"
grep -q "Version=\"$VERSION.0\"" "$WORK/msix/AppxManifest.xml" ||
	{
		echo "error: version substitution failed; is the manifest placeholder still 0.0.0.0?" >&2
		exit 1
	}

# --- pack --------------------------------------------------------------------
OUT="$HERE/octa-$VERSION.0.msix"
rm -f "$OUT"
"$MAKEMSIX" pack -d "$WORK/msix" -p "$OUT" >"$WORK/pack.log" 2>&1 ||
	{
		cat "$WORK/pack.log" >&2
		echo "error: makemsix pack failed." >&2
		exit 1
	}

# Unpack it again: this re-verifies every block-map hash, so a corrupt package
# is caught here instead of by Partner Center twenty minutes later.
"$MAKEMSIX" unpack -p "$OUT" -d "$WORK/verify" -ss -ac >"$WORK/verify.log" 2>&1 ||
	{
		cat "$WORK/verify.log" >&2
		echo "error: the package failed verification." >&2
		exit 1
	}
cmp -s "$WORK/extracted/octa.exe" "$WORK/verify/octa.exe" ||
	{
		echo "error: packaged octa.exe does not match the release binary." >&2
		exit 1
	}

echo
echo "Built and verified: $OUT"
echo "  $(du -h "$OUT" | cut -f1), unsigned (correct for Store upload)"
echo "  languages declared: $(grep -c '<Resource Language=' "$HERE/AppxManifest.xml")"
echo
echo "Upload it in Partner Center under Pakete."
