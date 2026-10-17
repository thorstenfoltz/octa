# Octa for macOS

A multi-format data viewer and editor for Parquet, CSV, JSON, XLSX, and more.

This build is for Apple Silicon (arm64) Macs.

## Install

Drag `Octa.app` into `/Applications` (or `~/Applications`).

The app is **ad-hoc signed but not notarised**, so macOS Gatekeeper refuses
the first launch of a copy downloaded with a browser. Two ways past it:

### Option A: Open Anyway

1. Double-click `Octa.app` and click **Done** (or **OK**) when macOS refuses.
2. Open **System Settings > Privacy & Security**, scroll down to
   *"Octa.app was blocked"* and click **Open Anyway**.
3. Confirm with your password and **Open Anyway** once more.

After that, double-clicking opens the app normally. (Control-click > Open did
this up to macOS 14; macOS 15 removed it.)

### Option B: clear the quarantine attribute

```bash
xattr -d com.apple.quarantine /Applications/Octa.app

# If that fails with "No such xattr" but the warning persists, or macOS
# says "Octa.app is damaged", use the recursive form once:
# xattr -cr /Applications/Octa.app
```

**Next time, skip all of this** with the one-line install, which downloads
with `curl` and so never triggers Gatekeeper:

```bash
curl -fsSL https://raw.githubusercontent.com/thorstenfoltz/octa/master/get-octa.sh | bash
```

## Run from the command line

```bash
/Applications/Octa.app/Contents/MacOS/octa [file]
```

Optionally link it onto your `PATH` (`/usr/local/bin` does not exist on a
fresh Apple Silicon Mac):

```bash
mkdir -p ~/.local/bin
ln -s /Applications/Octa.app/Contents/MacOS/octa ~/.local/bin/octa
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zprofile
```

## Uninstall

```bash
rm -rf /Applications/Octa.app ~/.local/bin/octa
```
