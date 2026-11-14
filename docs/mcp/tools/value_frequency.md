# `value_frequency`

Count how often each value appears in **one column** of a tabular file.
This is a `value_counts()` equivalent. Results are ordered most-frequent first.

## When to use

- Inspecting a categorical column's distribution.
- Finding the dominant (or rare) values before filtering.
- Histogramming a numeric column via Sturges binning.
- Finding the most common words or word pairs in a text column.

## Input schema

| Parameter   | Type    | Required? | Default      | Description                                                                            |
|-------------|---------|-----------|--------------|----------------------------------------------------------------------------------------|
| `path`      | string  | yes       | (no default) | Path to the file                                                                       |
| `column`    | string  | yes       | (no default) | Name of the column to count                                                            |
| `table`     | string  | no        | (no default) | Specific table for multi-table sources                                                 |
| `top_n`     | integer | no        | (all)        | Return only the N most frequent values / bins                                          |
| `bin`       | boolean | no        | `false`      | Group a numeric column into Sturges bins instead of raw values                         |
| `words`     | string  | no        | `off`        | `words` counts the words inside a text column, `pairs` the pairs of neighbouring words |
| `unlimited` | boolean | no        | `false`      | Lift the 2,000,000-row file-loader cap so the counts include every row in the file     |

## Response shape

```json
{
  "column_name": "country",
  "binned": false,
  "nulls": 12,
  "total_non_null": 9988,
  "unique_count": 47,
  "rows": [
    { "label": "US", "count": 4831 },
    { "label": "DE", "count": 1190 },
    { "label": "UK", "count": 1042 }
  ]
}
```

When `bin: true` on a numeric column, each `label` is a half-open range
like `[0.00, 5.00)` and `binned` is `true`. `unique_count` counts
distinct values (or bins) across the whole column even when `top_n`
shortens `rows`.

With `words` set to `words` or `pairs`, each `label` is a lower-case word
(or two words with a space between them). A word counts once per cell, so
`count` is the number of rows holding it; `nulls` counts the cells without a
word. Words are split at anything that is not a letter or digit, so text
written without spaces (Chinese, Japanese, Thai) is not split. Words are
always counted in the rows read, also on a database tab.

## Example calls

```json
{
  "name": "value_frequency",
  "arguments": { "path": "/tmp/users.parquet", "column": "country", "top_n": 10 }
}
```

```json
{
  "name": "value_frequency",
  "arguments": { "path": "/tmp/users.parquet", "column": "age", "bin": true }
}
```

## See also

- [`profile`](profile.md): stats for every column at once.
- [`find_duplicates`](find_duplicates.md): rows sharing key values.
- [Value Frequency](../../usage/value-frequency.md): the same compute
  in the GUI.
