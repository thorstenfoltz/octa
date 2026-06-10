# What's new in this branch

This branch (`feature/investigation`) adds three user-facing features to Octa,
plus a round of safety hardening and an Arrow/Parquet export fix.

## Summary tab

A one-click overview of the active table: one row of statistics per column,
the GUI counterpart of `octa --describe` and pandas' `df.describe()`.

- Open it from **Analyse -> Summary...**; it lands in a new tab named
  `Summary - <file>`.
- The statistics come from DuckDB's `SUMMARIZE`, so each column gets its
  type, `min` / `max`, approximate distinct count, mean / standard deviation
  and quartiles (numeric columns), non-null count, and null percentage.
- Unsaved cell edits are included: the summary describes the table as you
  currently see it, not the file on disk.
- The result is an ordinary, detached table tab with no source path, so it
  can be sorted, filtered, copied, and exported via **File -> Save As**, but
  can never overwrite the original file. Re-run it after further edits for a
  fresh snapshot.

## Search: filter or highlight

The toolbar search gains a mode toggle beside the search box that controls
how matches are shown.

- **Filter** (the default): non-matching rows are hidden, as before.
- **Highlight**: every row stays visible and matching cells are highlighted
  in place.

Set the default under **Settings -> Search & Editor -> Search result
display**; the toggle overrides it for the current session.

The table view honours the toggle. The text and tree views always highlight
(hiding free text or collapsing tree nodes is not meaningful there), which
means search now works in views where it previously had no effect:

- Jupyter notebooks
- JSON and YAML trees
- Markdown (preview and editor)
- the raw text editor
- the in-app documentation dialog

### Match count and navigation

When matches are highlighted, the search bar shows a `current / total` count
and two buttons to step through them. While the search box is focused,
**Enter** jumps to the next match and **Shift+Enter** to the previous one.
The view scrolls the current match into view and emphasises it: in a table
the match cell is selected, in text views the cursor moves to it, and in
trees and notebooks the relevant node or cell scrolls into view.

## Freeze columns

Pin key columns so they stay visible while scrolling a wide table
horizontally, like freezing panes in a spreadsheet.

- Right-click a column header and pick **Freeze columns up to here** to pin
  that column and every column to its left.
- A thin separator marks the boundary; the rest of the table scrolls
  underneath. **Unfreeze all columns** in the same menu reverts.
- The freeze is per tab and session-only, like column widths. If the window
  gets too narrow to keep the whole frozen band and still scroll, Octa
  temporarily pins fewer columns and restores the full band when there is
  room again.

## Safety hardening

- **Assistant writes are confined to the export directory.** The in-app chat
  assistant can only create files in the configured export folder (change it
  in Settings), with the single exception of writing back to a file you
  already have open in a tab. Writes anywhere else on disk are refused.
- **Tighter permissions** on sensitive config and chat-transcript files.
- **Verified updates:** release archives are checked against the published
  `SHA256SUMS` before the auto-update applies them.
- **Stricter archive extraction:** tighter size limits and path checks when
  extracting entries from `.zip` / `.tar` / `.tgz` archives.

## Arrow / Parquet export fix

Arrow and Parquet writes now cover every declared data type correctly,
including width-correct numeric builders and proper binary and date
handling, so schemas round-trip without mismatches.

---

📚 Full documentation: <https://thorstenfoltz.github.io/octa/>
