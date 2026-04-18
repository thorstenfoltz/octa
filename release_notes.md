## Features

- Add customizable keyboard shortcuts: all app actions are bound to user-editable `KeyCombo`s persisted in `settings.toml`; a new Settings panel lets you record, clear, and reset bindings
- Expand keyboard-driven editing with go-to-cell focus, direct cell edit, row insert/duplicate/delete, case transforms, and view-mode cycling
- Treat SQL query results as editable table updates rather than display-only output — metadata is preserved where possible, and results can be exported through the normal save pipeline
- Add confirmation dialogs before destructive reloads (reloading a modified file, disabling raw CSV alignment) to prevent accidental data loss
- Refresh the Settings UI with a branded header and clearer hover states

## Docs & Install

- README now documents running pre-built binaries directly (no install step); `install.sh` / `install.bat` are reserved for launcher integration or source builds
- Clarify Windows SmartScreen first-launch warning and `install.bat` behavior (Program Files + Start Menu shortcut, no PATH changes)
- Safer installer messaging on both platforms
