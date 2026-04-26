## Features

- Read SAS (`.sas7bdat`), SPSS (`.sav`, `.zsav`), and Stata (`.dta`) files via pure-Rust parsers — no system libraries required
- Write SPSS (`.sav`, `.zsav`) and Stata (`.dta`) files; SAS remains read-only since the SAS7BDAT format is proprietary and undocumented
- Add a `Random` icon-color mode and make it the new default: every Octa launch now picks a fresh color from the 12 built-in palettes
- Right-click → Copy / Copy All available across content views (raw text editor, SQL editor, PDF view, directory tree); selection-aware where the view has a TextEdit selection
- Add a macOS release artifact: tagged releases now publish a native Apple Silicon `.app` bundle alongside the existing platform builds
- Add macOS installation guidance covering unsigned-app startup behavior and Finder / terminal launch options
- Add cut support and configurable copy, cut, and paste shortcuts, with context-menu entries and table-side handling for remapped bindings
- Extend selection copying to sparse multi-cell ranges, preserving rectangular output with empty gaps where cells are not selected
- Add a toolbar toggle for treating the first row as headers, keeping selection state consistent when switching modes
- Expose toolbar actions for applying and clearing row, column, and cell marks from the current selection
- Improve the markdown view with in-page fragment navigation, basic HTML-to-Markdown preprocessing, and markdown copy support
- Add in-page fragment navigation to the JSON tree view

## Fixes

- Route copy, cut, and paste through a table-level handler so focused text editors (SQL editor, search bar, raw text) consume clipboard input first and no longer trigger table edits
- Stabilize JSON edit sizing so the editor no longer jumps as content changes
- Initialize a default selected cell when a table loads so keyboard navigation works immediately
- Format large row and column counts for readability in status and toolbar displays
- Update the shortcut documentation to reflect the now-customizable copy, cut, and paste bindings
