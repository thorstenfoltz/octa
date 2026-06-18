# Fill Missing Values

**Edit > Fill missing values...** replaces empty or null
cells in one column using a strategy you pick. Only empty/null cells are
changed; existing values are left alone. Apply is a single undo step.

## Strategies

| Strategy | What it does |
| --- | --- |
| Mean | Fill with the column's average (numeric columns only). |
| Median | Fill with the middle value (numeric columns only). |
| Mode | Fill with the most common value. |
| Constant | Fill with a fixed value you type. |
| Forward fill | Copy the nearest non-empty value from above. |
| Backward fill | Copy the nearest non-empty value from below. |

A strategy that doesn't fit the data (for example Mean on a text column)
shows an inline error and changes nothing. The operation respects
read-only mode.

## Command line and assistant

Also available as `octa --impute COL=STRATEGY` (see the
[`--impute`](../cli/impute.md) reference) and as the
[`fill_missing`](../mcp/tools/fill_missing.md) MCP / assistant tool.
