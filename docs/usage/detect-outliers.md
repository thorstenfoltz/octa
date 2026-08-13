# Detect Outliers

**Analyse > Detect outliers...** highlights numeric values that sit far from
the rest of their column, painting each flagged cell **orange** so unusual
readings stand out.

## Methods

- **IQR (interquartile range)** - flags cells outside
  `[Q1 - k*IQR, Q3 + k*IQR]`. The usual `k` is `1.5`.
- **Z-score (standard deviations)** - flags cells more than `k` standard
  deviations from the mean. The usual `k` is `3`.

Tick the columns to scan (numeric columns are pre-selected) and set `k`,
then press **Detect**. Columns with fewer than four numbers are skipped.

## What Detect does

Choose under **When done**:

- **Highlight outlier cells** - paints each flagged cell **orange**. This is
  **per tab and session-only**: it never changes the data, only how it is
  shown, and **Clear highlight** removes it. Manual colour marks, conditional
  colours, and validation highlights all take priority over the orange.
- **Add an is_outlier column** - appends a boolean `is_outlier` column that
  is `true` for every row holding at least one flagged value. This is a real,
  undoable edit (Ctrl+Z) you can save, sort, or filter on.

## Stepping through flagged cells

With **Highlight** chosen, <kbd>F10</kbd> jumps to the next flagged cell
and <kbd>Shift</kbd>+<kbd>F10</kbd> to the previous one, wrapping at the
ends, with a `Problem 3 of 27` counter in the status bar. The same keys
also cover [data validation](data-validation.md) violations, and rows
hidden by the current filter are skipped.

## Command line and assistant

Also available as `octa --outliers` (see the
[`--outliers`](../cli/outliers.md) reference) and as the
[`detect_outliers`](../mcp/tools/detect_outliers.md) MCP / assistant tool.
