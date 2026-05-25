# Troubleshooting

When things don't work. This page covers install-time and
day-to-day issues that aren't specific to the
[MCP server](mcp/troubleshooting.md).

## Install / first launch

### Windows SmartScreen prompt

Octa is **not code-signed**, so on first launch Windows SmartScreen
shows *"Windows protected your PC"* with only a **Don't run**
button visible.

**Fix**: click **More info** in the dialog, then **Run anyway**.
Subsequent launches open without the prompt.

If you want to permanently mark the binary as trusted, right-click
`octa.exe` → **Properties** → check **Unblock** at the bottom of
the General tab → OK.

### macOS: developer cannot be verified

The full message is *"Octa.app cannot be opened because the
developer cannot be verified"*. Octa is **ad-hoc signed but not
notarized**, so macOS Gatekeeper quarantines the bundle on first
launch.

**Fix Option A: right-click + Open**:

1. Control-click `Octa.app` in Finder.
2. Choose **Open**.
3. Click **Open** in the confirmation dialog.

macOS remembers the choice for that copy of the app, so
double-click works after this.

**Fix Option B: strip the quarantine attribute**:

```bash
xattr -d com.apple.quarantine /Applications/Octa.app
```

### macOS: app reported as damaged

The full message is *"Octa.app is damaged and can't be opened.
You should move it to the Trash."* with only a **Move to Trash**
button visible.

**The app is not actually damaged.** This is macOS Gatekeeper's
strictest quarantine response for downloaded binaries that aren't
notarized. It's particularly common when the file was downloaded
with `curl` / `wget` from outside a browser, transferred via
AirDrop between Macs, or moved out of `~/Downloads`. The binary
on disk is byte-identical to what's published on the [releases
page](https://github.com/thorstenfoltz/octa/releases).

**Fix: recursively clear the quarantine attribute**:

```bash
xattr -cr /Applications/Octa.app
```

`-cr` clears every extended attribute recursively (the bundle has
several internal files that each carry the quarantine flag, so the
non-recursive form from the previous section isn't enough here).
Double-click after running this and the app opens normally.

If you'd rather verify the download first, compare the binary
against the SHA-256 listed on the release page before clearing
the quarantine.

### Linux: build fails on system libraries

Linker errors mentioning Xlib / GTK / fontconfig mean system
development headers are missing. On Debian / Ubuntu:

```bash
sudo apt-get install -y \
  libgtk-3-dev libxcb-render0-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev libssl-dev \
  libfontconfig1-dev libfreetype6-dev
```

On Fedora:

```bash
sudo dnf install gtk3-devel libxcb-devel libxkbcommon-devel \
  openssl-devel fontconfig-devel freetype-devel
```

On Arch:

```bash
sudo pacman -S gtk3 libxcb libxkbcommon openssl fontconfig freetype2
```

### octa command not found

The binary isn't on your `PATH` after install. Three options:

- Move the binary to a directory already on `PATH`:
  `/usr/local/bin/octa` (system-wide) or `~/.local/bin/octa`
  (user-local, but make sure `~/.local/bin` is in your `PATH`).
- Add the binary's directory to `PATH` via your shell's rc file
  (`.bashrc`, `.zshrc`).
- Invoke Octa with its full path: `/path/to/octa file.parquet`.

On Windows, the `install.bat` script adds Octa to your user `PATH`.

## Opening files

### No reader available

The error reads *"no reader available for FILE"*, meaning Octa
doesn't recognise the extension. Three things to try:

- Check the extension spelling. `.parquet`, not `.parq`. `.csv`,
  not `.csv.txt`.
- Add the extension to [**Settings → Performance → Open as text**](reference/settings.md#performance)
  for log / config files Octa doesn't ship a reader for.
- Force open as text: rename the file with a `.txt` extension, or
  open via the [Raw view](usage/view-modes/raw-text.md)
  ([`octa --schema FILE`](cli/schema.md) won't work but the GUI
  can open anything via the text-reader fallback).

### Raw view fallback with an orange banner

When the format-specific reader errors out, Octa falls back to
plain-text rendering in the [Raw view](usage/view-modes/raw-text.md).
The orange banner at the top of the editor shows the parse error,
which is your debugging starting point.

Common causes per format:

- **CSV / TSV**: unusual quoting or escape that the reader can't
  guess. Switch the Raw view's [Quote / Escape
  combos](reference/csv-quote-escape.md) to match the file's
  actual convention.
- **JSON**: malformed JSON. Try a JSON linter to spot the exact
  error position.
- **YAML**: indentation issue or tab character in a structural
  position.
- **TOML**: type-mismatched value or unrecognised key.

### Large file takes forever to open

Streaming readers (Parquet, CSV, TSV) cap initial-load at 1M
rows (configurable under [**Settings → Performance → Initial-load
row cap**](reference/settings.md#performance)). Non-streaming
formats (Excel, SQLite, JSON) load the whole file before showing
anything.

If you only need a quick look, the CLI is faster, since there's
no GUI render path, so the cost is just the read plus a small
projection:

```bash
octa --schema huge.parquet           # streams up to the initial-load cap, projects schema
octa --head huge.csv -n 10           # streams up to the cap, then truncates to N
```

For non-streaming formats (Excel, JSON, SPSS, Stata) the whole file
still loads to populate the table, so [`octa --schema`](cli/schema.md)
and [`--head`](cli/head.md) on those formats aren't materially
faster than the GUI's load step.

### Valid JSON still fails to parse

The error reads *"Failed to parse as JSON"* even though the file is
syntactically valid JSON. Octa's reader expects either:

- A top-level **array** of objects (each object becomes a row).
- A top-level **object** with array-typed values (parallel arrays
  become rows).

If your top-level JSON is a single non-array object, Octa wraps it
in a one-row table; that usually works but can surprise. Files
with nested objects deeper than 2 levels get stringified into
`Nested` cells; expand them via the
[**JSON Tree** view](usage/view-modes/json-and-yaml-tree.md).

## The GUI feels slow

### Scrolling stutters during background load

The background loader is filling rows. Watch the status bar: the
busy spinner runs while rows are streaming in. Once
`bg_loading_done` flips, scrolling smooths out.

For files over a few GB, raising [**Settings → Performance →
Initial-load row cap**](reference/settings.md#performance) uses more
memory but eliminates the background-load lag on initial scroll.

### Raw view feels laggy

Syntect tokenisation is doing real work per keystroke in the
[Raw view](usage/view-modes/raw-text.md). Three options:

- Reduce [**Settings → Performance → Syntax-highlight size cap**](reference/settings.md#performance)
  (set to 0 for fully plain monospace).
- Switch the file off the Raw view (cycle with the
  [`CycleViewMode` shortcut](reference/shortcuts.md#view), F4 by
  default, if applicable).
- For huge CSV/TSV files in Raw view, accept the one-shot prompt
  that asks to disable column alignment + coloring for that tab.

### High RAM use

The biggest consumers:

- **Initial-load row cap**: 1M rows of a wide table can be
  hundreds of MB.
- **Background row buffer** for streaming loads: temporary, but
  large during the load.
- **Image previews** ([EPUB](usage/view-modes/epub-reader.md)):
  uploaded textures stay in memory until the tab closes.
- [**Compare view**](usage/view-modes/compare.md): both sides
  loaded fully into memory.

Close unused tabs (or reopen with the lower cap) to free memory.

## Saving fails

### Database save: schema mismatch

You renamed, added, or removed a column. Database saves reject
schema changes by design (see [Saving →
Databases](usage/saving.md#database-files-sqlite-duckdb-geopackage)):
the in-memory column set must match the on-disk schema.

To do a schema change:

1. Make the change in another tool (`sqlite3` CLI, DBeaver, etc.):

   ```sql
   ALTER TABLE users ADD COLUMN signup_source TEXT;
   ```

2. Reopen the database in Octa; the new schema is now the baseline.
3. Edit and save.

### Format does not support writing

The error reads *"format X does not support writing"*, meaning
you're trying to **Save As** into a read-only format (SAS, RDS,
HDF5, NetCDF, EPUB, GeoJSON). Pick a writable format instead (CSV,
Parquet, JSON, SQLite, etc.). The
[supported-formats matrix](getting-started/supported-formats.md)
lists which formats round-trip through write.

### CSV save lost my dates

Promoted Date columns write back as **canonical ISO `YYYY-MM-DD`**
on text outputs (CSV / JSON / etc.), regardless of the source
format. If you want to preserve the on-disk format (e.g.
`DD.MM.YYYY`):

1. After loading, click **Dismiss** on the date-format-change
   banner. That reverts the column to its source-string form.
2. Save. The original strings are written back unchanged.

See [Date Inference](reference/date-inference.md) for the full
mechanics.

## Tabs / open files

### Closed a tab by accident

**Ctrl+Shift+T** (the
[`ReopenLastClosedTab` shortcut](reference/shortcuts.md#file-operations))
reopens the most recently closed tab.

### Tabs disappear after restart

Octa doesn't persist open tabs across sessions yet. Workaround:
re-open them from **File → Recent Files** (the entry count is
configurable in
[Settings → Files](reference/settings.md#files)).

### Folder sidebar shows the wrong directory

**File → Close Directory** hides the
[folder sidebar](usage/tabs-and-sidebar.md#the-folder-sidebar)
without closing any tabs. Then **File → Open Directory…** to point
at a different folder.

## Keyboard shortcuts not working

### A shortcut does nothing

Common causes:

- A **text editor** is focused
  ([Raw view](usage/view-modes/raw-text.md),
  [SQL editor](usage/sql.md), search box, rename
  dialog). Most table-level shortcuts gate on "no TextEdit focused"
  so the keystroke can reach the editor. Click outside the editor
  and try again.
- [**Read-only mode**](usage/editing.md#read-only-mode) is on
  (**F8** toggle). Many editing shortcuts no-op while read-only.
- The action is **rebound or unbound**. Check
  [**Settings → Shortcuts**](reference/settings.md#shortcuts) or
  the full
  [keyboard shortcuts reference](reference/shortcuts.md).

### Two actions claim the same combo

The Settings dialog flags conflicts when you record a binding. If
two actions slipped through with the same combo somehow, the
[**Settings → Shortcuts**](reference/settings.md#shortcuts) grid
highlights both rows; fix one and Apply.

## Updates

### Update check failed

Octa checks GitHub releases via HTTPS. Common blockers:

- No internet (or firewall blocking github.com).
- Corporate proxy. Octa doesn't yet honor `HTTPS_PROXY` env vars;
  this is a known limitation.
- Rate-limiting from the GitHub API (very rare for individual users).

You can always download the latest binary manually from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and
swap in place.

### Linux: update needs elevation

The update dialog reports *"Needs elevation to install"*, meaning
Octa was installed to a system directory (`/usr/local/bin/octa`)
but the auto-updater can't write there as your user. Two options:

- Re-install to a user-local prefix: `./install.sh ~/.local`.
- Run the update step with `sudo`.

The dialog explains and links to both paths.

## Still stuck?

- For **MCP server issues** see the dedicated [MCP
  troubleshooting page](mcp/troubleshooting.md).
- For **bugs** (Octa behaving differently than this page describes),
  open an issue at
  [github.com/thorstenfoltz/octa/issues](https://github.com/thorstenfoltz/octa/issues).
  Include your Octa version (`octa --version`), OS, and exact
  reproduction steps.
- For **feature requests** or general questions, the
  [Discussions tab](https://github.com/thorstenfoltz/octa/discussions)
  is the right place.

## See also

- [Installation](getting-started/installation.md) covers Linux
  system libs, Windows manifest, and macOS Apple Silicon notes.
- [MCP troubleshooting](mcp/troubleshooting.md) covers the stdio
  handshake, client config, and schema errors.
- [Settings reference](reference/settings.md) shows where the
  config file lives if you want to inspect or reset it.
