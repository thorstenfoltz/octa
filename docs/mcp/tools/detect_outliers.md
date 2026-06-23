# `detect_outliers`

Flag numeric cells that sit far from the rest of their column. Returns the
coordinates of the flagged cells; it does not change the data.

## When to use

- Data-quality screening for suspicious numbers.
- Locating extreme values before charting or aggregation.

## Input schema

| Parameter  | Type     | Required? | Default         | Description                                        |
|------------|----------|-----------|-----------------|----------------------------------------------------|
| `path`     | string   | no\*      | (no default)    | Path to the file (omit when `open_tab` is set)     |
| `open_tab` | string   | no        | (no default)    | Operate on an open GUI tab (`@active` or tab name) |
| `table`    | string   | no        | (no default)    | Specific table for multi-table sources             |
| `columns`  | string[] | no        | all columns     | Columns to check                                   |
| `method`   | string   | no        | `iqr`           | `iqr` or `zscore`                                  |
| `k`        | number   | no        | 1.5 IQR / 3.0 z | Threshold multiplier                               |

\* `path` or `open_tab` is required.

**IQR** flags cells outside `[Q1 - k*IQR, Q3 + k*IQR]`. **Z-score** flags
cells whose value is more than `k` standard deviations from the mean.
Columns with fewer than four numeric values are skipped.

## Response shape

```json
{
  "flagged": [ { "row": 41, "column": "amount" }, … ],
  "count": 3,
  "method": "iqr",
  "k": 1.5
}
```

`row` is the zero-based row index; `column` is the column name.

## Example call

```json
{
  "name": "detect_outliers",
  "arguments": {
    "path": "/data/sales.parquet",
    "columns": ["amount"],
    "method": "zscore",
    "k": 3.0
  }
}
```

## See also

- [`profile`](profile.md): per-column summary statistics.
- [`value_frequency`](value_frequency.md): value distribution and binning.
