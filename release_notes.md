## Features

- New **read-only Column Inspector** with per-column type and quick stats (min, max, not-null, all-unique). Multi-select rows, sort A→Z / Z→A without touching
the actual table order, double-click a row to jump to that column in the underlying table, or right-click → **Copy column** to grab every value of a single inspector column (e.g., a one-shot export of every column name or every type)
- **Alphabetical column sort** is now a toolbar action and is undo/redo-safe, so reordering is reversible
- **0-byte files** open as a friendly placeholder view instead of failing with a schema error
- **Deep Sea** and **Frost** themes added; selected-cell contrast, numeric alignment, and layout spacing tightened across light and dark palettes
- Parquet, ORC, and RDS readers now decode additional binary and string-backed Arrow types (`FixedSizeBinary`, `BinaryView`, `Utf8View`, R `raw` vectors), so the **Binary display mode** (Binary / Hex / UTF-8 Text) applies to more files

## Fixes

- Selected numeric cells stay readable — the previous "blue text on blue selection background" collision in Deep Sea and the default dark theme is gone
- Column Inspector right-click menu no longer becomes unresponsive after the first use
- Settings → Shortcuts grid no longer hides Record / Clear / Reset buttons behind matching striped-row backgrounds

## Documentation

- In-app documentation and package descriptions updated to reflect the current set of supported formats and database-save behavior
