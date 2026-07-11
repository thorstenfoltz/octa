# View Modes

A "view mode" is the way Octa displays the file's content. The
[**Table view**](../table-view.md) is the default for almost every
format, but some files are better viewed in their native shape:
[Markdown](markdown.md) as rendered HTML,
[Jupyter notebooks](notebook.md) with cell outputs,
[EPUB](epub-reader.md) as flowing text,
[GeoJSON on a map](map.md).

Switch view modes via the **View** menu in the toolbar. Only modes
applicable to the current file are enabled.

A few file types open in a non-Table view that suits them better: a
`.json` file opens in the [JSON Tree](json-and-yaml-tree.md), and a
`.yml` / `.yaml` file opens in [Raw Text](raw-text.md). You can always
switch from the View menu; this just picks a sensible starting point.
JSONL and every other format still open in Table view.

<!-- SCREENSHOT: view-menu.png: View menu open in the toolbar, showing the radio buttons for Table / Raw Text / Markdown / Notebook / EPUB Reader / Map / JSON Tree / YAML Tree / Compare / Read-only mode. -->
![View menu](../../assets/screenshots/view-menu.png){ .screenshot-placeholder }

## All view modes at a glance

| View mode                              | Available for                        | Read-only?                    |
|----------------------------------------|--------------------------------------|-------------------------------|
| [**Table**](../table-view.md)          | Every format                         | No (editing fully supported)  |
| [**Raw Text**](raw-text.md)            | Anything Octa can read as UTF-8 text | No                            |
| [**Markdown**](markdown.md)            | `.md`, `.markdown`, `.mdown`, `.mkd` | No (edit + preview)           |
| [**Notebook**](notebook.md)            | `.ipynb`                             | Yes                           |
| [**JSON Tree**](json-and-yaml-tree.md) | `.json`, `.jsonl`                    | Edit keys + values in place   |
| [**YAML Tree**](json-and-yaml-tree.md) | `.yaml`, `.yml`                      | Same as JSON Tree             |
| [**EPUB Reader**](epub-reader.md)      | `.epub`                              | Yes                           |
| [**Map**](map.md)                      | `.geojson`                           | Yes (geometry rendering only) |
| [**Compare**](compare.md)              | Any file (compared against another)  | Yes (it's a diff viewer)      |

## Open as... (files with a misleading extension)

Which view modes a file offers depends on how it was parsed, and Octa
parses by extension. A `.log` file that actually holds JSON is read as
plain text, so the JSON Tree never appears in the View menu.

Two menu entries fix that, depending on whether the file is open yet:

- **File → Open as...** for a file you have not opened. Pick the format,
  then pick **one or more files** in the file dialog. The dialog is
  deliberately unfiltered (it shows every file), because the files worth
  opening this way are exactly the ones whose extension Octa would
  otherwise route somewhere unhelpful. Each file opens in its own tab.
- **View → Reopen as** for the file already in the current tab. It
  re-reads that one file in place.

Both offer the same formats:

| Choose            | Reads the file as                              |
|-------------------|------------------------------------------------|
| **JSON**          | A single JSON document (tree view available)   |
| **JSON Lines**    | One JSON object per line (the usual log shape) |
| **CSV** / **TSV** | Delimited text, into a table                   |
| **YAML**          | A YAML document                                |
| **TOML**          | A TOML document                                |
| **XML**           | An XML document                                |
| **Markdown**      | CommonMark, with the rendered preview          |
| **Plain text**    | Raw text, no parsing                           |

Pick **JSON** for that `.log` and it parses as JSON, tree view and all,
exactly as though the file had been named `.json`. Log files that hold one
JSON object per line want **JSON Lines** instead.

This changes only how Octa reads the file. Nothing on disk is renamed or
rewritten. Reopening re-reads from disk, so any unsaved edits in that tab
are discarded, and if the content does not parse as the chosen format the
tab is left exactly as it was, with the error shown in the status bar.

## Cycling view modes

**F4**
([`CycleViewMode`](../../reference/shortcuts.md#view)) advances through the modes available for the current tab in this order:

```
Table → Raw → Markdown → Notebook → JsonTree → YamlTree → EpubReader → Map → Compare
```

> **Note**: Neither [Chart](../chart.md) nor the [SQL panel](../sql.md)
> are view modes. Chart opens in its own tab via **Analyse → Chart...**
> or <kbd>F5</kbd>; the SQL panel docks alongside the table via
> **Analyse → SQL** or <kbd>Ctrl</kbd>+<kbd>J</kbd>. The cycling list
> above only walks true view modes, and cycling inside a chart tab is
> a no-op.

Modes that don't apply to the current file are skipped silently,
so for a CSV, **F4** cycles between Table and Raw only.

## Read-only mode

**F8**
([`ToggleReadOnly`](../../reference/shortcuts.md#view)) toggles a session-only read-only state independent
of view mode. Every editing path short-circuits:

- Double-click on a cell doesn't enter edit mode.
- The [raw text editor](raw-text.md) renders non-interactive.
- Shortcuts for [Insert / Delete row](../editing.md#inserting-rows),
  [Mark](../colour-marking.md), Undo, Redo all no-op.

The status bar shows a `[Read-only]` pill while active. A one-shot
notice explains the mode the first time you toggle it.

Read-only is not persisted, so it resets every time you launch
Octa.

## Per-mode references

- [Raw Text](raw-text.md) shows the file contents as plain text,
  with syntax highlighting for source languages and column
  alignment for CSV/TSV.
- [Markdown](markdown.md) renders CommonMark with a Preview /
  Split / Edit toggle.
- [JSON & YAML Tree](json-and-yaml-tree.md) is a collapsible tree
  view with in-place key + value editing.
- [Notebook](notebook.md) renders Jupyter notebooks with cell
  outputs.
- [EPUB Reader](epub-reader.md) is a chapter-by-chapter reading
  view with embedded images.
- [Map](map.md) is a slippy-map view for GeoJSON feature geometries.
- [Compare](compare.md) is a side-by-side diff of two files (text
  or row hash).

The Table view itself is covered under [Usage → Table View](../table-view.md).
The [SQL panel](../sql.md) and the [Chart](../chart.md) tab live
under the **Analyse** menu and are documented on their own pages.

## See also

- [Table view](../table-view.md) is the default view for almost
  every tabular file.
- [Supported formats](../../getting-started/supported-formats.md)
  lists which formats default to which view mode.
- [Settings → Appearance](../../reference/settings.md#appearance)
  covers the default theme, font size, and view-mode defaults.
- [Keyboard shortcuts](../../reference/shortcuts.md) lists F4 (cycle
  view modes) and F8 (toggle read-only) among others.
