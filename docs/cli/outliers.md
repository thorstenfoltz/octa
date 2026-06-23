# `--outliers`

Flag numeric outlier cells in a file and print the flagged coordinates to
stdout. The input file is never modified.

```
octa --outliers FILE [--outlier-method iqr|zscore] \
     [--outlier-cols COL[,COL,...]] [--outlier-k K] [-f tsv|json|csv]
```

## Methods

- `iqr` (default) - flag cells outside `[Q1 - k*IQR, Q3 + k*IQR]`.
- `zscore` - flag cells more than `k` standard deviations from the mean.

`--outlier-k` sets the threshold multiplier (default `1.5` for IQR, `3.0`
for z-score). `--outlier-cols` limits the scan to named columns (default:
all columns). Columns with fewer than four numbers are skipped.

## Examples

```
octa --outliers sales.csv
octa --outliers sales.csv --outlier-method zscore --outlier-k 3 --outlier-cols amount
```

## See also

- [Detect Outliers](../usage/detect-outliers.md) (GUI) and the
  [`detect_outliers`](../mcp/tools/detect_outliers.md) MCP tool.
