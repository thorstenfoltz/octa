# `--impute`

Fill missing/empty cells in one or more columns of a file and print the
result to stdout. The input file is never modified.

```
octa --impute COL=STRATEGY [--impute COL2=STRATEGY2 ...] FILE [-f tsv|json|csv]
```

Each `--impute` flag takes a `COL=STRATEGY` pair; repeat the flag to fill
several columns in one run. The data file is the positional argument.

## Strategies

| Strategy | What it does |
| --- | --- |
| `mean` | Column average (numeric columns only). |
| `median` | Middle value (numeric columns only). |
| `mode` | Most common value. |
| `ffill` | Forward fill: nearest non-empty value from above. |
| `bfill` | Backward fill: nearest non-empty value from below. |
| `const:VALUE` | A fixed value, e.g. `const:0` or `const:unknown`. |

Only empty/null cells are changed. A strategy that doesn't fit the column
(e.g. `mean` on text) is an error.

## Examples

```
octa --impute temperature=median readings.csv -f csv > filled.csv
octa --impute region=const:unknown --impute score=mean data.parquet
```

## See also

- [Fill Missing Values](../usage/fill-missing-values.md) (GUI) and the
  [`fill_missing`](../mcp/tools/fill_missing.md) MCP tool.
