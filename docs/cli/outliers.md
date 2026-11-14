# `--outliers`

Flag numeric outlier cells in a file and print the flagged coordinates to
stdout. The input file is never modified.

```
octa --outliers FILE [--outlier-method iqr|zscore|neighbours] \
     [--outlier-cols COL[,COL,...]] [--outlier-k K] \
     [--order-by COL] [--outlier-window POINTS] [--outlier-same-weekday] \
     [-f tsv|json|csv]
```

## Methods

- `iqr` (default) - flag cells outside `[Q1 - k*IQR, Q3 + k*IQR]`.
- `zscore` - flag cells more than `k` standard deviations from the mean.
- `neighbours` - for a series over time: flag cells more than `k` robust
  deviations from the median of the values around them, in the order of
  `--order-by`. Finds a dip inside a trend that the other two miss.

`--outlier-k` sets the threshold multiplier (default `1.5` for IQR, `3.0`
for z-score and neighbours). `--outlier-cols` limits the scan to named columns (default:
all columns). Columns with fewer than four numbers are skipped.

| Flag                     | Default | What it does                                                                  |
|--------------------------|---------|-------------------------------------------------------------------------------|
| `--order-by`             | none    | `neighbours`: the date, time or number column that orders the rows (required) |
| `--outlier-window`       | 7       | `neighbours`: values compared on each side                                    |
| `--outlier-same-weekday` | off     | `neighbours`: compare a value only with those on the same weekday             |

The three belong to the `neighbours` method and are refused with the other
two. The rows need not be sorted by `--order-by`.

## Examples

```
octa --outliers sales.csv
octa --outliers sales.csv --outlier-method zscore --outlier-k 3 --outlier-cols amount
octa --outliers daily.csv --outlier-method neighbours --order-by day --outlier-cols visits --outlier-same-weekday
```

## See also

- [Detect Outliers](../usage/detect-outliers.md) (GUI) and the
  [`detect_outliers`](../mcp/tools/detect_outliers.md) MCP tool.
