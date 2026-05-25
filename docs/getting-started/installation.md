# Installation

Octa runs on **Linux**, **macOS** (Apple Silicon), and **Windows**,
but it is primarily developed for Linux.
Pre-built binaries are published on the
[releases page](https://github.com/thorstenfoltz/octa/releases) for
each platform. No installer is required to run Octa, since every
download is a single executable that works in place.

The platform-specific install scripts (`install.sh`, `install.bat`)
are optional conveniences that wire Octa into your application
launcher, Start Menu, etc.

## Linux

### Pre-built binary

Download the Linux archive from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and
extract it. The binary is portable, so drop it anywhere and run:

```bash
chmod +x octa
./octa                          # run from anywhere
# or place it on your PATH:
mv octa ~/.local/bin/octa
```

### AppImage

Octa is also published as an
[AppImage](https://appimage.org/) for users who prefer a single
self-contained file. Download `Octa-*-x86_64.AppImage` from the
[releases page](https://github.com/thorstenfoltz/octa/releases),
make it executable, and run it directly:

```bash
chmod +x Octa-*-x86_64.AppImage
./Octa-*-x86_64.AppImage
```

The AppImage bundles GTK and every native dependency, so it works on
any reasonably recent Linux distribution without needing the system
packages listed under [Build from source](#build-from-source).

!!! note "FUSE-less hosts"

    AppImages mount themselves via FUSE. If your distribution doesn't
    ship `libfuse2` (Ubuntu 24.04 dropped it, some minimal containers
    don't include it), run the AppImage with the built-in extract-and-run
    fallback instead:

    ```bash
    ./Octa-*-x86_64.AppImage --appimage-extract-and-run
    ```

### Install via `install.sh` (recommended)

If you want Octa to appear in your application launcher with an icon
and file associations, run the install script after extracting the
archive:

```bash
# System-wide (installs to /usr/local, requires sudo)
sudo ./install.sh

# User-local (no sudo)
./install.sh ~/.local
```

The script installs:

- the `octa` binary to `<prefix>/bin/`
- the icon to `<prefix>/share/icons/hicolor/scalable/apps/octa.svg`
- a desktop entry to `<prefix>/share/applications/octa.desktop`
- the third-party license bundle (`THIRD_PARTY_LICENSES.md` +
  `licenses/`) next to the binary, satisfying the Apache-2.0 / MIT /
  BSD / OFL attribution requirements.

The script auto-detects whether a pre-built `octa` binary is next to
it. If yes, it copies that. If not, it builds from source (requires
the [Rust toolchain](#build-from-source), see below).

### Uninstall

```bash
sudo ./uninstall.sh
# or, matching your install prefix:
./uninstall.sh ~/.local
```

### Arch Linux

Octa is available on the AUR in two flavours, install using paru or yay:

```bash
paru -S octa              # builds from source
paru -S octa-bin          # installs the pre-built binary
```

### Build from source

Building requires a C compiler and a few GTK / X11 / fontconfig
libraries. **`asciidoctor`** is optional, but `install.sh` uses it
to render the man page from `docs/cli/octa.1.adoc` so `man octa`
works; without it the install succeeds but the man page is skipped.

```bash
sudo apt-get install -y \
  libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev libssl-dev \
  libfontconfig1-dev libfreetype6-dev \
  asciidoctor
```

Then with [`rustup`](https://rustup.rs/) installed:

```bash
git clone https://github.com/thorstenfoltz/octa.git
cd octa
cargo build --release
# binary at target/release/octa
```

---

## macOS

The release artifact targets **Apple Silicon (arm64)** Macs.

### Pre-built `.app` bundle

Download the macOS archive from the
[releases page](https://github.com/thorstenfoltz/octa/releases), unzip
it, and drag `Octa.app` into `/Applications` (or `~/Applications`).

Octa is **ad-hoc signed but not notarized**, so macOS Gatekeeper
will warn the first time you launch it with *"Octa.app cannot be
opened because the developer cannot be verified"*. Two ways around
it (and a third fix for the
[*"is damaged"*](../troubleshooting.md#macos-app-reported-as-damaged)
variant):

=== "Option A: right-click → Open"

    Control-click `Octa.app` in Finder, choose **Open**, then confirm
    in the Gatekeeper dialog. macOS remembers the decision for that
    copy of the app, so subsequent double-clicks open it normally.

=== "Option B: strip the quarantine attribute"

    ```bash
    # See whether the bundle is quarantined
    find /Applications -maxdepth 1 -name "Octa.app" -exec xattr {} \;

    # Strip it
    xattr -d com.apple.quarantine /Applications/Octa.app
    ```

    If the strip above succeeds but the warning persists, or you see
    *"Octa.app is damaged and can't be opened"* (only "Move to Trash"
    button), use the recursive form:

    ```bash
    xattr -cr /Applications/Octa.app
    ```

### Run from the command line

```bash
/Applications/Octa.app/Contents/MacOS/octa [file]
```

Optionally symlink it onto your `PATH`:

```bash
ln -s /Applications/Octa.app/Contents/MacOS/octa /usr/local/bin/octa
```

### Build from source

```bash
brew install harfbuzz freetype gtk+3
cargo build --release
# binary at target/release/octa
```

### Uninstall

```bash
rm -rf /Applications/Octa.app
```

---

## Windows

### Pre-built binary

Download `octa.exe` from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and
run it directly, no installation step needed. Place it wherever you
like (`Desktop`, `C:\Tools\`, etc.) and double-click to launch.

!!! warning "First-launch SmartScreen prompt"

    Octa is **not code-signed**, so on first launch Windows shows
    *"Windows protected your PC"*. Click **More info** → **Run
    anyway**. Subsequent launches open without the prompt.

### Install via `install.bat`

If you want Octa in your Start Menu and on `PATH`:

1. Right-click `install.bat` and choose **Run as administrator**.
2. The script copies `octa.exe` to `C:\Program Files\Octa`, adds the
   directory to your user `PATH`, and creates a Start Menu shortcut.
3. Restart any open terminals to pick up the `PATH` change.

### Run from the command line

```cmd
octa.exe path\to\file.parquet
```

### Uninstall

1. Delete `C:\Program Files\Octa`.
2. Remove `C:\Program Files\Octa` from your `PATH` (**Settings →
   System → About → Advanced system settings → Environment
   Variables**).
3. Delete the Start Menu shortcut at `%APPDATA%\Microsoft\Windows\Start
   Menu\Programs\Octa.lnk`.

---

## Verify the install

After installation, open a terminal and run:

```bash
octa --help
```

You should see Octa's usage text with the action flags
(`--schema`, `--head`, `--convert`, `--sql`, `--mcp`, etc.) listed. If you
get *"command not found"*, the binary isn't on your `PATH`, so
either move it somewhere on `PATH` or invoke it via its full path.

To launch the GUI:

```bash
octa                            # empty window
octa path/to/file.parquet       # open a file in a tab
octa file1.csv file2.json       # open multiple files (one tab each)
```

These action flags run a single operation and exit
without launching the GUI. See the
[CLI overview](../cli/index.md) for details.

On Linux, the install scripts (`install.sh` and the AUR
`octa` / `octa-bin` packages) also install a man page, so
**`man octa`** gives you the full reference at a terminal. The
same content lives on this site under
[CLI → Man Page](../cli/man-page.md).

## Where settings are stored

| Platform | Path                                                                               |
|----------|------------------------------------------------------------------------------------|
| Linux    | `$XDG_CONFIG_HOME/octa/settings.toml` (defaults to `~/.config/octa/settings.toml`) |
| macOS    | `~/Library/Application Support/Octa/settings.toml`                                 |
| Windows  | `%APPDATA%\Octa\settings.toml`                                                     |

The settings file is generated on first launch with defaults.

## See also

- [First Steps](first-steps.md) is a short tour for someone who has
  just launched Octa.
- [Supported formats](supported-formats.md) lists every format Octa
  reads and the subset it can write back.
- [Settings reference](../reference/settings.md) documents every
  preference the config file can carry.
- [Troubleshooting](../troubleshooting.md) covers install-time
  gotchas (system libs, gatekeeper warnings, missing fonts).

[Continue with First Steps :material-arrow-right:](first-steps.md){ .md-button }
