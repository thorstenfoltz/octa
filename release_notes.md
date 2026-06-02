# Release notes: `feature/several-small-improvements`

A batch centred on **internationalization**, the GUI now speaks 13
languages, plus a set of quality-of-life loading and analysis features
and a headless container image for the CLI and MCP server.

## Internationalization (13 languages)

The desktop interface can now be shown in any of 13 Latin-script
languages, switchable **live** under **Settings -> Appearance ->
Language** with no restart.

- **Coverage.** Toolbar menus, every prompt and dialog, the status bar,
  the SQL and multi-search panels, the cell / row / column right-click
  menus, the Chart view, and the whole Settings dialog including its
  hover tooltips. Enum dropdown values (search mode, mark colour, binary
  display, rounding, and so on) translate too.
- **Languages.** English (master), German, Spanish, French, Italian,
  Dutch, Portuguese, Polish, Swedish, Danish, Norwegian, Finnish, and
  Turkish.
- **Hand-rolled catalogs.** `src/i18n.rs` plus one TOML file per
  language under `locales/`, embedded at compile time. `t(key)` looks
  up the active language and falls back to English, then to the key
  itself, so an untranslated string degrades gracefully instead of
  showing a blank. A parity test enforces that every English key exists
  in every other locale.
- **Machine-generated, flagged for review.** The non-English catalogs
  are machine-translated and may be refined over time.
- **Out of scope on purpose.** Cyrillic, CJK, and right-to-left scripts
  are not offered yet (they need bundled fonts and, for RTL, layout
  work). Technical identifiers (format names, DB engines, theme names,
  reader error strings) deliberately stay in English. Number formatting
  (decimal mark and digit grouping) remains a separate **Number style**
  setting, independent of the UI language.

## Content-based format detection

Octa no longer trusts the file extension alone. When an extension is
missing, wrong, or unrecognised, it inspects the file's content to pick
a reader (`src/formats/sniff.rs`).

- **Magic bytes** identify binary formats regardless of name (a Parquet
  file called `export.bin`, an extensionless SQLite database, a
  ZIP-based archive).
- **Structure probes** recognise text formats (a `.txt` that is really
  JSON, a delimited file whose extension disagrees).
- The sniffer is consulted before the plain-text fallback, and Octa also
  **retries via the sniffed reader** when the extension-chosen reader
  errors, so renamed and mislabelled files usually just open as the
  right thing.

## Malformed CSV / TSV repair (opt-in)

A new **Offer repair on malformed files** setting (default **off**, in
**Settings -> File-Specific**) opts into a repair prompt for delimited
files that read but look broken.

- Detects bad text encoding, a leading BOM, control characters, a
  delimiter that disagrees with the extension, and ragged rows
  (`csv_reader::analyze_delimited`).
- The prompt lists what was detected and previews the repaired result;
  you choose **Repair and open**, **Open without repair**, or
  **Cancel** (`src/app/dialogs/repair_file.rs`).
- Repair re-decodes the text, re-detects the delimiter, and strips stray
  markers. **The file on disk is never modified**, only what Octa loads
  into memory. CSV/TSV only.

## Date / Time calculation

A new **Edit -> Date/Time calculation...** dialog derives a column from
date, datetime, or duration values without a formula or SQL
(`src/data/time_calc.rs`, `src/app/dialogs/time_calc.rs`).

- **Five operations.** Difference between two dates (in a chosen unit),
  add / subtract time (negative subtracts; month/year add clamps the
  day to the target month), convert duration units, extract a
  component (year, month, day, hour, minute, second, weekday), and
  **Unix timestamp / date** conversion in either direction at
  second / millisecond / microsecond / nanosecond precision (UTC epoch;
  nanosecond values keep full precision via 128-bit integer math).
- Materialises a **new column** and leaves the source columns untouched,
  like Insert Column and formulas. Text columns are read through the
  same date-inference parser the table uses; cells that aren't valid
  dates or numbers are skipped with a banner.

## Docker / Podman

A headless container image for the command-line actions and the MCP
server (`Dockerfile`, `.dockerignore`).

- Multi-stage build: a `rust:1-bookworm` stage compiles the release
  binary, then it is copied into a `distroless/cc-debian12` runtime. The
  GUI library stack (GTK / X11 / Wayland) is dropped because the
  headless paths never load it, so the runtime ships only the data
  engine's dependencies.
- `docker run -v "$PWD:/data" octa --schema /data/f.parquet` for one-shot
  CLI actions; `docker run -i ... octa --mcp` to host the MCP server over
  stdio. Podman-compatible with the same Dockerfile.

## Editable Jupyter notebooks

The Notebook view's source cells are now editable. Click into any code
or markdown cell and type; code cells keep their syntax highlighting
while you edit. Edits flow through the normal table machinery, so undo
(Ctrl+Z), the modified marker, and Save all work, and the output cells
stay read-only.

Saving an edited notebook now **preserves cell outputs**,
execution counts, and per-cell metadata. The writer reuses the original
`.ipynb` as a template and overwrites only the source (and cell type) of
each cell, so re-saving no longer wipes a notebook's outputs the way it
did before. An edited cell keeps its prior output, matching Jupyter's
behaviour when you change a cell without re-running it.

## Fixed-width (FWF) reader

Octa now opens fixed-width text files (`.fwf`, `.prn`). These have no
delimiter, so Octa infers the column boundaries by sampling the leading
lines and finding the character positions that are blank in every line,
and treats the first line as the header. Read-only and best-effort:
cleanly aligned exports parse well; columns whose values run together
cannot be split.

## CLI / MCP additions

- **`--tail` / `--sample`.** `octa --tail FILE -n N` prints the last N
  rows; `octa --sample FILE -n N --seed S` prints a reproducible random
  sample. Both mirror `--head` and are also exposed as the MCP tools
  `tail` and `sample`.
- **`--diff` (row-level data diff).** `octa --diff A B` compares two
  files row by row (whole-row content, columns positional) and prints
  the rows unique to each side with a leading `status` column, plus a
  shared-row summary on stderr. Also available as the MCP tool
  `diff_tables`. This complements the existing `--compare-schemas`,
  which only diffs column metadata.
- **MCP data writing (`write_table` / `edit_table`).** The MCP server
  can now write, not just read. `write_table` takes inline `columns` and
  array-of-arrays `rows` (the same shape `read_table` returns) and writes
  them to any writable format by output extension, with `create`,
  `overwrite`, and `append` modes. `edit_table` edits an existing file in
  place, setting cells (by column index or name), inserting rows, and
  deleting rows; SQLite and DuckDB sources keep their diff-based save, so
  only the rows that actually changed are written back. Together they
  close the gap left by `convert` (transcode only) and `run_sql`
  `write_to` (DuckDB / SQLite only).

## Bug fixes

- **DuckDB timestamps no longer show as raw epoch integers.** The DuckDB
  reader and the SQL engine surfaced `TIMESTAMP`, `DATE`, and `TIME`
  values as their raw integer count (e.g. a microsecond timestamp showed
  as `1769975775172766`) instead of a readable value. Both now format
  them like the Parquet / Avro / ORC readers do (`2026-02-01 19:56:15…`),
  via shared helpers so the `.duckdb` file path, the SQL view, and the
  MCP `run_sql` tool all agree. Affected `.duckdb`/`.ddb` files and any
  SQL result with a temporal column.
- **Autofit columns no longer clips long numbers.** "Fit all columns"
  (Ctrl+Shift+W) and the header-seam double-click measured each cell's
  raw value, ignoring the thousands separators the table actually paints.
  A long integer like `2206240000000000001` was sized for 19 characters
  but rendered as `2,206,240,000,000,000,001` (25), so the trailing
  digits were cut off. The best-fit width now measures the same formatted
  text the cell displays, honouring the thousands-separator setting, the
  English / European number style, and any per-column rounding format.

## Documentation

GitHub Pages (the mkdocs site) gained:

- A **Languages** reference page and a **Date / Time calculation** usage
  guide, both added to the nav.
- New sections in **Supported formats** covering content-based detection
  and malformed-file repair, plus a **Fixed-width (FWF)** row and
  caveat section.
- New rows in the **Settings** reference for **Language** and **Offer
  repair on malformed files**, plus refreshed feature highlights on the
  home page. The **Docker / Podman** CLI page documents the container
  image.
- An updated **Notebook view** page documenting editable source cells
  and output-preserving saves.
- New CLI pages for **`--tail`**, **`--sample`**, and **`--diff`**, and
  new MCP tool pages for **`tail`**, **`sample`**, **`diff_tables`**,
  **`write_table`**, and **`edit_table`**, all added to the nav. The man
  page (`octa.1.adoc`) and its site mirror list the new actions and the
  full twenty-tool MCP set.
