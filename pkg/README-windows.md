# Octo for Windows

A multi-format data viewer and editor for Parquet, CSV, JSON, Excel, and more.

## Install

Run `install.bat` as Administrator (right-click, "Run as administrator").

This will:
- Copy `octo.exe` to `C:\Program Files\Octo`
- Add it to your user PATH
- Create a Start Menu shortcut

You may need to restart your terminal for PATH changes to take effect.

## Run without installing

Double-click `octo.exe` or run from the command line:

```
octo.exe [file]
```

## Uninstall

1. Delete `C:\Program Files\Octo`
2. Remove `C:\Program Files\Octo` from your PATH (Settings > System > About > Advanced system settings > Environment Variables)
3. Delete the Start Menu shortcut at `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Octo.lnk`
