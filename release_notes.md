## Features

- **Cycle view modes with one keypress.** New `CycleViewMode` shortcut (default **F4**, remappable in **Settings → Shortcuts**) advances through the modes available for the current tab — Table, Raw, Markdown, Notebook, PDF, JSON Tree — skipping anything that doesn't apply to the file. Suppressed when a TextEdit has focus so F4 inside the SQL/raw editor types normally.
- **Documentation reorganized as a sidebar + content pane.** `Help → Documentation` now opens with a category list on the left (Getting Started, Navigation, Editing, Search & Replace, View Modes, SQL, Saving, Settings reference, Shortcuts, …) and the corresponding section rendered on the right. Section selection persists within a session. The Shortcuts section continues to auto-generate from the user's current bindings.
- **Quote-aware coloring in the raw CSV/TSV view.** Per-column colors now follow logical fields, not raw delimiter splits. A quoted cell like `"1,2,3,4,5"` keeps its color across the embedded commas — the layouter walks `split_delimited_line_ranges` instead of `String::split`. Aligned output also re-quotes any cell whose content contains the delimiter so the formatted text round-trips through the same tokenizer.
- **JSON arrays of objects expand to indexed columns.** When an array contains nested objects (e.g. `{"data":{"items":[{"x":{"y":1}}]}}`), every leaf path is unrolled to its own column with `[N]` index — `data.items[0].x.y` rather than a single JSON-string blob. Arrays of pure primitives (`["a","b"]`) still ride along as one JSON-string cell. Records-detection now only treats a top-level array as the rows when it contains objects, so a `tags` field stays anchored to its column instead of producing rows of `value`.
- **Date-format-change banner.** When date inference promotes a column under a non-canonical layout (e.g. stored as `02.05.2026`, displayed as `2026-05-02`), a dismissible banner appears above the central panel naming the affected columns and their detected source format. **Dismiss** reverts the promotion: the column returns to its on-disk strings and the type drops back to `Utf8`. The banner can be silenced globally under **Settings → File-Specific → Warn on date format change**.
- **Slow-CSV prompt offered on demand, not at load.** The first time you actually open the raw view of a CSV/TSV larger than 10 MB, a one-shot per-file prompt asks whether to disable per-column coloring (the slow path) for this file. Align Columns stays available either way, and the choice never modifies your settings.
- **Clickable email in About.** The author email in **Help → About** is now a `mailto:` link (`thorsten.foltz@live.com`) and the author line shows just the name.

## Fixes

- **Live PDF text follows table edits.** The PDF reader now uses `mupdf` for both bitmap rendering and per-page text extraction, producing a 3-column table (`page`, `line`, `text`). The PDF view re-derives its selectable text frames from the live table on every render, so edits in the table view are reflected immediately under each page image. (Page bitmaps still come from the on-disk PDF — they're frozen until you reload.)
- **Quote/escape changes no longer re-read from disk.** Switching the raw view's Quote or Escape combo while align mode is on re-formats from a cached in-memory snapshot of the original content instead of going back to disk. Same for the un-align action.
- **Larger documentation, less scrolling.** The old single-page documentation dialog has been split into typed sections (see Features above), removing duplicated content and surfacing format-specific notes under a single "View Modes" heading.

## Removals

- **PDF write support has been dropped.** The previous writer regenerated the entire PDF from extracted text, losing layout, typography, and embedded objects. PDF tabs are now read-only — Save / Save As are hidden for them. Use the table view to inspect and the source application to edit. The `printpdf` and `pdf-extract` dependencies have been removed.

## Documentation

- `CLAUDE.md` updated for the quote-aware raw view, slow-CSV prompt, view-mode shortcut, date warning, PDF page-aware reader, and JSON array-of-objects handling.

## Internals

- **Dependency refresh.** Bumped `strum 0.26 → 0.28`, `calamine 0.26 → 0.34`, `rust_xlsxwriter 0.82 → 0.94`, `quick-xml 0.37 → 0.39` (with the new `BytesText::decode` + `quick_xml::escape::unescape` split), `hdf5-reader 0.2 → 0.4` (drop the now-unparameterized `Dataset` lifetime), `dta 0.4 → 0.5`, `rfd 0.15 → 0.17`, `toml 0.8 → 1` (route document parses through `toml::from_str`), `rand 0.8 → 0.10` (`IndexedRandom` trait + `rand::rng()`), `rusqlite 0.32 → 0.39`, `apache-avro 0.17 → 0.21`. The egui ecosystem (0.31) and arrow stack (54) stay where they are pending dedicated migration passes — both have widespread breaking-API churn that didn't fit this batch.
