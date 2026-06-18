# Drop Duplicate Rows

**Edit > Drop duplicate rows...** (Ctrl+Shift+H) removes repeated rows from
the active table in one step.

## How it works

Tick the columns that make up the **key**. Two rows count as duplicates
when all their checked columns are equal. With every column ticked (the
default) only exact whole-row repeats are removed; tick just one column to
collapse rows that share that value.

Choose whether to **keep the first** or **keep the last** occurrence of
each key, then press Apply. The rest are removed as a single undoable step
(Ctrl+Z restores them all), and the status bar reports how many rows went.

Values are compared as text, so `1` (integer) and `1.0` (float) are not
treated as the same. The operation respects read-only mode.

## Command line and assistant

The same engine is available as `octa --dedupe` (see the
[`--dedupe`](../cli/dedupe.md) reference) and as the
[`drop_duplicates`](../mcp/tools/drop_duplicates.md) MCP / assistant tool.
To **report** duplicates instead of removing them, see
[Find Near-Duplicates](find-near-duplicates.md) or the search bar's Find
duplicates highlight.
