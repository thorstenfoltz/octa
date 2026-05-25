# Value Frequency

The Value Frequency dialog answers "what are the most common values in
this column?" — a one-click equivalent of pandas'
`df['x'].value_counts()` for the active table.

<!-- SCREENSHOT: value-frequency-overview.png — Value Frequency dialog showing top 50 values in a categorical column with counts and percentages. -->
![Value Frequency](../assets/screenshots/value-frequency-overview.png){ .screenshot-placeholder }

## Opening the dialog

| Path                          | Notes                                                                                                                                  |
|-------------------------------|----------------------------------------------------------------------------------------------------------------------------------------|
| **Right-click column header** | "Value frequency…" entry in the column-header context menu.                                                                            |
| **Keyboard shortcut**         | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> Targets the column of the currently selected cell, or column 0 when no cell is selected. |

## What it shows

Three columns per row:

| Column    | Meaning                                                                |
|-----------|------------------------------------------------------------------------|
| **Value** | The distinct cell value (or the range, if numeric binning is enabled). |
| **Count** | How many rows have this value (or fall in this bin).                   |
| **%**     | `count / total_non_null * 100`, to one decimal place.                  |

Rows are sorted by **Count** descending; ties broken alphabetically by
**Value** for a deterministic ordering. The footer reports total
distinct values, total non-null cells, and the null count for the
column.

## Top-N presets

| Preset                                              | Effect                                                                      |
|-----------------------------------------------------|-----------------------------------------------------------------------------|
| **Top 20** / **Top 50** / **Top 100** / **Top 500** | Show the N most common values. Default is **Top 50**.                       |
| **All**                                             | Show every distinct value. Watch the row count on high-cardinality columns. |

The selected preset persists per tab, so reopening the dialog on the
same tab remembers your choice.

## Numeric binning (Sturges)

For numeric columns (`Int*`, `Float*`), check **Bin numeric values
(Sturges)** to group values into ranges instead of counting each raw
value. The bin count uses Sturges' formula
(`ceil(1 + log₂(n))`), clamped to `[5, 30]` buckets.

Bin labels show `[lo, hi)` half-open intervals (closed on the right
for the last bin so the maximum lands somewhere). When every value in
the column is the same, the result collapses to a single bucket.

Non-finite values (`NaN`, `±Inf`) and accidental non-numeric cells
inside a numeric column show up as separate rows alongside the bins —
useful for catching type drift in messy data.

The checkbox is hidden for non-numeric columns.

## Acting on a frequency row

Right-click any row (when binning is off) to get:

- **Copy value** — puts the raw value on the clipboard.
- **Filter table to this value** — adds a column filter restricting the
  active table to rows where this column equals the picked value. The
  Excel-style column-filter chip appears in the status bar; see
  [Column Filter](search-and-filter.md#column-filter) to clear it.

## Copy as TSV

The **Copy as TSV** button at the bottom puts the entire visible table
on the clipboard as three tab-separated columns: `<column>`, `count`,
`percent`. Useful for pasting into a spreadsheet or another Octa tab
via the regular paste-into-cells path.

## See also

- [Column Inspector](column-inspector.md) — schema-level overview of
  every column at once.
- [Search & Filter](search-and-filter.md) — including the column
  filter that Value Frequency can populate.
- [Keyboard Shortcuts](../reference/shortcuts.md) — to rebind
  Ctrl+Shift+I.
