## Features

- **Column-wide date inference** on text-format loads (CSV, TSV, JSON, JSONL, Excel, XML, TOML, YAML, Markdown, DBF). Recognizes `YYYY-MM-DD`, `YYYY/MM/DD`, `DD.MM.YYYY`, `DD-MM-YYYY`, `DD/MM/YYYY`, `MM-DD-YYYY`, `MM/DD/YYYY`, plus `[ T]HH:MM[:SS[.fff]]` datetime variants. A column is only promoted when *every* non-null value parses under one consistent layout, so mixed columns stay as text. When the same values are consistent with both DD/MM and MM/DD (e.g. `02/03/2024`), a per-column modal asks **European / US / Leave as text** before changing anything.
- **Open multiple files at once.** `File → Open` accepts multi-select and the CLI accepts multiple paths — each file lands in its own tab. Files queue in the background and pause for table-picker / date-ambiguity dialogs so multi-table DBs and ambiguous columns still get resolved one at a time.
- **Best-fit column width on double-click.** Double-clicking the seam between two column headers auto-sizes the leftward column to fit its content (header + up to 5,000 sampled rows). Single-drag still resizes manually.
- **Raw CSV view: quote-aware alignment.** New **Quotes** (Double / Single / Either / None) and **Escape** (Doubled `""` / Backslash `\"` / None) combos in the raw view toolbar, alongside the existing delimiter selector. A delimiter inside `"a,b"` now stays in one field instead of splitting the row. Defaults match RFC 4180.

## Fixes

- **Marked numeric cells stay readable in dark themes.** Numbers on a colored mark now switch to a high-contrast text color the same way selected cells already did — the "yellow background, accent-blue digits" collision is gone for every mark color (red / orange / yellow / green / blue / purple) and every dark theme.
- **Konami easter-egg arrows render correctly.** Replaced the Unicode arrow glyphs (which showed as tofu boxes in egui's default font) with painted vector triangles in a foreground banner. The arrows always render regardless of the active font.
- **Rainbow theme is actually rainbow.** Ten palette slots — accent, accent-hover, headers, primary/secondary text, borders, alt-row stripe, selection, warning — cycle through HSV at staggered phase offsets, so the whole window glides through the spectrum on a ~10s cycle. Backgrounds stay near-black so contents remain readable.
- **JSON tree handles large files.** Dropped a per-frame deep clone of the parsed JSON, cached the file's max-depth at load, and virtualized the scroll area with `ScrollArea::show_rows` over a flattened row list — an 11 MB JSON now scrolls and expands smoothly.
- **Windows self-update no longer wedges on a stale `.old.exe`.** Startup now best-effort cleans up leftovers (`.old.exe` / `.update.exe`) from a previous update, and if a leftover *can't* be removed during a new update, the updater surfaces an actionable error pointing the user at the file instead of failing silently.

## Documentation

- `CLAUDE.md` extended with new sections covering column-wide date promotion, multi-file open, raw CSV quote/escape modes, and best-fit column width.
