# `correlation`

Compute a **pairwise correlation matrix** over the numeric columns of a file or
open tab. Read-only analytics (stays available under `--mcp-read-only`).

## When to use

- Spot related numeric features before modelling.
- Quick "which columns move together?" check.

## Input schema

| Parameter   | Type   | Required? | Default      | Description                                                 |
|-------------|--------|-----------|--------------|-------------------------------------------------------------|
| `path`      | string | yes*      | (no default) | Path to the file (omit when `open_tab` is set)              |
| `open_tab`  | string | no        | (no default) | Operate on an open GUI tab (`@active` or a tab name)        |
| `table`     | string | no        | (no default) | Specific table for multi-table sources                      |
| `method`    | string | no        | `pearson`    | `pearson` (linear) or `spearman` (monotonic, rank-based)    |
| `unlimited` | bool   | no        | `false`      | Lift the 5,000,000-row file-loader cap so every row is used |

Non-numeric columns are ignored. For each pair, only rows where **both** values
are present are used.

## Response shape

```json
{
  "columns": ["height", "weight", "age"],
  "matrix": [
    [1.0, 0.82, 0.15],
    [0.82, 1.0, 0.10],
    [0.15, 0.10, 1.0]
  ]
}
```

`matrix[i][j]` is the correlation of `columns[i]` with `columns[j]`. A
coefficient is `null` when undefined (fewer than two paired rows, or zero
variance).

## Example call

```json
{
  "name": "correlation",
  "arguments": {
    "path": "/tmp/measurements.csv",
    "method": "spearman"
  }
}
```

## See also

- [`profile`](profile.md): per-column summary statistics.
- [`value_frequency`](value_frequency.md): distribution of a single column.
