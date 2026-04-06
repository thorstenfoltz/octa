# Octo for Linux

A multi-format data viewer and editor for Parquet, CSV, JSON, Excel, and more.

## Install

Run the install script (installs to `/usr/local` by default, requires sudo):

```bash
sudo ./install.sh
```

To install to a custom prefix (e.g. `~/.local` for user-local, no sudo needed):

```bash
./install.sh ~/.local
```

This installs:

- Binary to `<prefix>/bin/octo`
- Icon to `<prefix>/share/icons/hicolor/scalable/apps/octo.svg`
- Desktop entry to `<prefix>/share/applications/octo.desktop`

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
./octo [file]
```

## Arch Linux

Octo is available on the AUR as `octo` (source) and `octo-bin` (pre-compiled).
