# `count_rows`

Count rows in a tabular file. Loads the table and reports the row
count.

## When to use

- "How big is this file?" prompts.
- Sanity check before deciding whether to call
  [`read_table`](read_table.md) with the default cap (1000) or with
  `limit: 0`.

## Input schema

| Parameter   | Type   | Required? | Default | Description                                                                        |
|-------------|--------|-----------|---------|------------------------------------------------------------------------------------|
| `path`      | string | yes       | —       | Path to the file                                                                   |
| `table`     | string | no        | —       | Specific table for multi-table sources                                             |
| `unlimited` | bool   | no        | `false` | Lift the 5,000,000-row file-loader cap so the count reflects every row in the file |

## Response shape

```json
{
  "row_count": <n>,
  "initial_load_capped": <bool>,
  "initial_load_cap": <n>
}
```

### `initial_load_capped`

For streaming formats (Parquet, CSV, TSV), Octa applies an
**initial-load row cap** (default 5,000,000) at load time, after
which [`read_table`](read_table.md) and friends stop pulling more rows.

`count_rows` works on the same loaded table, so on those streaming
formats it counts the **loaded rows**, not necessarily every row in
the source file. When the loaded count hits the cap, this flag is
`true` and `initial_load_cap` echoes the current cap so the model
knows the count is an underestimate. Pass `unlimited: true` to
disable the cap for this call and get the true total.

For non-streaming formats (Excel, SQLite, JSON, etc.), the whole
file is loaded so the count is exact and `initial_load_capped` is
`false`.

## Example calls

### Count an Excel file's rows

```json
{
  "name": "count_rows",
  "arguments": { "path": "/tmp/quarterly.xlsx" }
}
```

Response (small file, exact count):

```json
{
  "row_count": 4823,
  "initial_load_capped": false,
  "initial_load_cap": 5000000
}
```

### Count rows in a SQLite table

```json
{
  "name": "count_rows",
  "arguments": {
    "path": "/data/app.sqlite",
    "table": "orders"
  }
}
```

Response:

```json
{
  "row_count": 4891002,
  "initial_load_capped": false,
  "initial_load_cap": 5000000
}
```

(SQLite is non-streaming; even though the count exceeds the cap,
the cap doesn't apply here.)

### Count a huge Parquet file (cap applied)

```json
{
  "name": "count_rows",
  "arguments": { "path": "/tmp/events-2024.parquet" }
}
```

Response (cap was hit):

```json
{
  "row_count": 5000000,
  "initial_load_capped": true,
  "initial_load_cap": 5000000
}
```

A model seeing `initial_load_capped: true` should mention to the
user that the count is an underestimate, and offer to re-call with
`unlimited: true`:

```json
{
  "name": "count_rows",
  "arguments": {
    "path": "/tmp/events-2024.parquet",
    "unlimited": true
  }
}
```

Response (cap lifted; whole file read):

```json
{
  "row_count": 47832104,
  "initial_load_capped": false,
  "initial_load_cap": 18446744073709551615
}
```

Note that [`run_sql`](run_sql.md) with `SELECT count(*) FROM data`
is subject to the **same** initial-load cap unless it is also called
with `unlimited: true`. Parquet files with very many row groups
(> 32,767) fall back to a DuckDB-backed reader automatically and
open without manual recompaction.

## Why a dedicated tool

`count_rows` exists separately from `read_table` because:

- The response is small (~50 bytes) regardless of file size.
- It surfaces the streaming cap, which is invisible from a
  `read_table` response.
- Some models prefer a dedicated tool for "how many rows" over
  parsing a `read_table` response.

## See also

- [`run_sql`](run_sql.md) is also subject to the initial-load cap;
  useful for aggregation but not a workaround for the cap.
- [Limits & truncation](../limits-and-truncation.md) covers the same
  initial-load cap that affects this tool.
