# `schema`

Return the column schema of a file: names + data types only, with
no rows in the response. Cheap discovery step before deciding
whether to call [`read_table`](read_table.md) or
[`run_sql`](run_sql.md).

## When to use

- Answering "what columns does this file have?"
- Probing an unknown file before deciding what to do with it.
- Building a SQL query, since knowing the column types up-front
  lets Claude pick the right operators.

## Input schema

| Parameter | Type   | Required? | Default      | Description                            |
|-----------|--------|-----------|--------------|----------------------------------------|
| `path`    | string | yes       | (no default) | Path to the file                       |
| `table`   | string | no        | (no default) | Specific table for multi-table sources |

## Response shape

```json
{
  "columns": [
    { "name": "<column>", "type": "<arrow_type>" },
    …
  ],
  "column_count": <n>
}
```

The schema entries are typed in the same Arrow-derived vocabulary
[`read_table`](read_table.md) uses: `Int64`, `Float64`, `Utf8`,
`Boolean`, `Date32`, `Timestamp(Microsecond, None)`, `Binary`, etc.

## Example calls

### Basic schema lookup

```json
{
  "name": "schema",
  "arguments": { "path": "/tmp/sales.parquet" }
}
```

Response:

```json
{
  "columns": [
    { "name": "region", "type": "Utf8" },
    { "name": "quarter", "type": "Utf8" },
    { "name": "amount", "type": "Float64" },
    { "name": "order_id", "type": "Int64" }
  ],
  "column_count": 4
}
```

### Schema of a specific table

```json
{
  "name": "schema",
  "arguments": {
    "path": "/data/app.sqlite",
    "table": "users"
  }
}
```

## Performance

`schema` shares its load path with [`read_table`](read_table.md): it
routes through the same `FormatRegistry::reader_for_path` and reads
the file into a `DataTable`, then projects only the column metadata
into the response. Compared to `read_table` it saves the
serialisation cost of every row, not the load cost.

- For streaming formats (Parquet, CSV, TSV) the server's
  initial-load cap is applied at load time;
  even a multi-GB Parquet is usually sub-second.
- For non-streaming formats (Excel, JSON, Stata, etc.) the
  whole file is loaded. On small files this is fast; on multi-GB
  Excel workbooks `schema` is no cheaper than `read_table` with
  `limit: 1`.

## See also

- [`read_table`](read_table.md) returns schema + rows together.
- [`list_tables`](list_tables.md) returns every table's schema at
  once for multi-table databases.
