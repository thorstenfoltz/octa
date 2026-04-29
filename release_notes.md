## Features

- Apply a color to **every cell in a free multi-cell selection** from the Edit > Mark menu — previously only the anchor cell was colored
- New **Mark** keyboard shortcut (default Ctrl+M, remappable) that paints the current selection (rows, columns, multi-cell, or single cell) with the configured default color
- New **Default mark color** setting (Settings > Table, Yellow by default) — drives the Mark shortcut
- Surface **Undo / Redo in the Edit menu**, with the current keybinding shown next to each entry and disabled state when the stacks are empty
- Move **Undo / Redo into the customizable shortcut system** (Settings > Shortcuts) so the default Ctrl+Z / Ctrl+Y bindings can be rebound; they now appear in the auto-generated shortcut documentation
- Add two new UI themes — **Manga** (cream-paper light theme with sakura-pink and sky-blue accents on bold ink-black text) and **Gentleman** (deep walnut and burgundy dark theme with champagne-gold accents on warm parchment text)
- New **R Dataset** reader for `.rds` files (read-only) — opens `data.frame` and `tibble` saved with `saveRDS()`; supports logical, integer, double, character, factor, `Date`, and `POSIXct` columns, with `NA` rendered as empty cells. Powered by the pure-Rust `rds2rust` crate, no R runtime required
- New **DBF / dBase** reader and writer — opens the `.dbf` sidecars of shapefile bundles, government and legacy data exports, and dBase III/IV/FoxPro files. Round-trip via the pure-Rust `dbase` crate
- **HDF5 compound datasets** now decode real values — DataFrames written by pandas/PyTables (`df.to_hdf(..., format="table")`) previously rendered every cell as the literal placeholder `(compound)`; numeric and fixed-string fields now show actual data

## Fixes

- Make the **Settings dialog draggable** — it now opens centered but can be moved freely, matching the Documentation dialog's behavior; the About dialog gets the same treatment
- Replace the awkward Settings **font size drag-arrow** with a dropdown listing every integer 8–32 pt
- Strengthen window **close-X hover highlighting** with an accent-tinted fill and thicker stroke so the button reads clearly
- Widen the status-bar **Go to R:C** input from 120 to 180 px so the hint text and short inputs are no longer clipped
- Scope **Ctrl+Z / Ctrl+Y** to the focused TextEdit (SQL editor, raw editor, search bar) so text-undo no longer triggers table undo

