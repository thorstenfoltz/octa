# Octa for Linux

A multi-format data viewer and editor for Parquet, CSV, JSON, Excel, and more.

## Install

Given no argument the install script uses `/usr/local` (`/usr` on Arch Linux),
and that needs root. It checks before copying anything, so a run without the
necessary rights stops with a message rather than half-installing.

```bash
sudo ./install.sh
```

To install to a custom prefix (e.g. `~/.local` for user-local, no sudo needed):

```bash
./install.sh ~/.local
```

This installs:

- Binary to `<prefix>/bin/octa`
- Icon to `<prefix>/share/icons/hicolor/scalable/apps/octa.svg`
- Desktop entry to `<prefix>/share/applications/octa.desktop`

## Uninstall

```bash
sudo ./uninstall.sh
```

Or with the same custom prefix used during install:

```bash
./uninstall.sh ~/.local
```

## Run without installing

```bash
./octa [file]
```

## Arch Linux

Octa is available on the AUR as `octa` (source) and `octa-bin` (pre-compiled).
