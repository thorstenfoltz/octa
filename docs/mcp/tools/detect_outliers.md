# `detect_outliers`

Flag numeric cells that sit far from the rest of their column, or from
their neighbours in a series over time. Returns the
coordinates of the flagged cells; it does not change the data.

## When to use

- Data-quality screening for suspicious numbers.
- Locating extreme values before charting or aggregation.
- Finding an unusual day in a time series that has a trend or a weekly rhythm.

## Input schema

| Parameter      | Type     | Required?  | Default                        | Description                                                |
|----------------|----------|------------|--------------------------------|------------------------------------------------------------|
| `path`         | string   | no\*       | (no default)                   | Path to the file (omit when `open_tab` is set)             |
| `open_tab`     | string   | no         | (no default)                   | Operate on an open GUI tab (`@active` or tab name)         |
| `table`        | string   | no         | (no default)                   | Specific table for multi-table sources                     |
| `columns`      | string[] | no         | all columns                    | Columns to check                                           |
| `method`       | string   | no         | `iqr`                          | `iqr`, `zscore` or `neighbours`                            |
| `k`            | number   | no         | 1.5 IQR / 3.0 z and neighbours | Threshold multiplier                                       |
| `order`        | string   | neighbours | (no default)                   | Column that orders the rows: a date, timestamp or number   |
| `window`       | integer  | no         | `7`                            | `neighbours`: values compared on each side                 |
| `same_weekday` | boolean  | no         | `false`                        | `neighbours`: compare only with values on the same weekday |

\* `path` or `open_tab` is required.

**IQR** flags cells outside `[Q1 - k*IQR, Q3 + k*IQR]`. **Z-score** flags
cells whose value is more than `k` standard deviations from the mean.
Columns with fewer than four numeric values are skipped.

**Neighbours** is for a series over time: each value is compared with the
`window` values before and after it in the order of `order`, and flagged
when it lies more than `k` robust deviations (1.4826 times the median
absolute deviation) from their median. It finds a dip inside a trend, which
the other two miss. `same_weekday` compares a Sunday only with Sundays.
`order`, `window` and `same_weekday` are refused with the other methods.

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
- In the app: [Detect Outliers](../../usage/detect-outliers.md)
- Command line: [`--outliers`](../../cli/outliers.md)
