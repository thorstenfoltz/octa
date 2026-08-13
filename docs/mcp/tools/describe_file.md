# `describe_file`

One-shot orientation snapshot of a tabular file: format, file size,
row count, column schema, and a small sample of rows. Replaces the
common three-call dance of `list_tables` → `schema` → `read_table`
when first meeting an unfamiliar file.

CLI mirror: [`octa --describe`](../../cli/describe.md).

## When to use

- The very first call when handed an unfamiliar file.
- Anywhere a quick "what is this?" answer is more useful than the
  full schema or row data.
- As a lighter-weight alternative to `profile` when statistics
  aren't needed.

## Input schema

| Parameter     | Type    | Required? | Default      | Description                                                                                                                                             |
|---------------|---------|-----------|--------------|---------------------------------------------------------------------------------------------------------------------------------------------------------|
| `path`        | string  | yes       | (no default) | Path to the file.                                                                                                                                       |
| `table`       | string  | no        | (no default) | Specific table for multi-table sources.                                                                                                                 |
| `sample_rows` | integer | no        | `5`          | Sample-row count. Clamped to `[0, 100]`.                                                                                                                |
| `unlimited`   | boolean | no        | `false`      | Lift the 5,000,000-row file-loader cap so the row count is exact.                                                                                       |
| `deep`        | boolean | no        | `false`      | Also report the file's physical layout: row groups, compression, encodings, column statistics, plus layout hints. Parquet only; ignored for `open_tab`. |

For multi-table sources called without `table`, the reader's default
behaviour applies, so call `list_tables` first if you're unsure.

## Response shape

```json
{
  "path": "/data/sales.parquet",
  "format_name": "Parquet",
  "file_size_bytes": 1048576,
  "table": null,
  "row_count": 47832,
  "initial_load_capped": false,
  "initial_load_cap": 5000000,
  "columns": [
    { "name": "id", "type": "Int64" },
    { "name": "region", "type": "Utf8" },
    { "name": "amount", "type": "Float64" }
  ],
  "column_count": 3,
  "sample_rows": [
    [1, "EU", 1234.5],
    [2, "US", 9876.0]
  ],
  "sample_row_count": 2,
  "cell_truncated": false
}
```

Sample rows obey the server's per-cell byte cap; any cell that
overflows is replaced with a `[truncated: N bytes; cap M bytes ...]`
marker and `cell_truncated` flips to `true`.

## Example call

```json
{
  "name": "describe_file",
  "arguments": {
    "path": "/data/sales.parquet",
    "sample_rows": 3
  }
}
```

Multi-table source:

```json
{
  "name": "describe_file",
  "arguments": {
    "path": "/data/app.sqlite",
    "table": "users"
  }
}
```

Force an exact row count on a very large file:

```json
{
  "name": "describe_file",
  "arguments": {
    "path": "/data/huge.parquet",
    "unlimited": true,
    "sample_rows": 0
  }
}
```

## Deep inspection

With `deep: true` the response gains an `internals` object:

```json
{
  "internals": {
    "facts": {
      "format": "Parquet",
      "rows": "4200000",
      "row_groups": "12",
      "created_by": "parquet-cpp-arrow version 15.0.0",
      "compressed_bytes": "812004221",
      "uncompressed_bytes": "3104882190",
      "bloom_filters": "0"
    },
    "hints": [
      "No column statistics. Query engines cannot skip row groups without min/max values, so every read touches the whole file."
    ]
  }
}
```

`facts` values are strings so the shape stays stable whatever the
format. `hints` is a possibly empty array of plain-language warnings
about small row groups, missing statistics, or missing compression.

Use this when asked why a file is large or slow. Formats other than
Parquet report their size and say they have no inspectable internal
structure, which is an answer rather than an error.

## See also

- [`schema`](schema.md), [`count_rows`](count_rows.md),
  [`read_table`](read_table.md): the three calls `describe_file`
  collapses.
- [`profile`](profile.md): the heavier-weight cousin, returns
  per-column statistics via DuckDB SUMMARIZE.
- [`octa --describe`](../../cli/describe.md), the CLI mirror.
