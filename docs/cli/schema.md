# `octa --schema`

Print the column schema of a tabular file: column name + data type,
nothing else.

## Synopsis

```bash
octa --schema FILE [-f tsv|json|csv]
```

| Flag             | Required | Default      | Meaning                                                        |
|------------------|----------|--------------|----------------------------------------------------------------|
| `--schema FILE`  | yes      | (no default) | Path to the file to inspect.                                   |
| `-f`, `--format` | no       | `tsv`        | Output format. See [CLI overview](index.md#output-formatting). |

## What it prints

Two-column output:

| Column | Meaning                                       |
|--------|-----------------------------------------------|
| `name` | The column's name from the file               |
| `type` | The column's data type, in Octa's type system |

Octa's type strings are Arrow-derived: `Int8`, `Int16`, `Int32`,
`Int64`, `Float32`, `Float64`, `Utf8`, `LargeUtf8`, `Boolean`,
`Date32`, `Timestamp(Microsecond, None)`, `Binary`, `LargeBinary`,
etc. These map cleanly to most other type systems.

## Examples

### TSV (default)

```bash
$ octa --schema sales.parquet
name      type
region    Utf8
quarter   Utf8
amount    Float64
order_id  Int64
```

### JSON

```bash
$ octa --schema sales.parquet -f json
[
  { "name": "region", "type": "Utf8" },
  { "name": "quarter", "type": "Utf8" },
  { "name": "amount", "type": "Float64" },
  { "name": "order_id", "type": "Int64" }
]
```

Piping into `jq` works as you'd expect:

```bash
octa --schema sales.parquet -f json | jq -r '.[] | "\(.name): \(.type)"'
# region: Utf8
# quarter: Utf8
# amount: Float64
# order_id: Int64
```

### CSV

```bash
$ octa --schema sales.parquet -f csv
name,type
region,Utf8
quarter,Utf8
amount,Float64
order_id,Int64
```

## Notes

- **Multi-table sources** (SQLite, DuckDB, GeoPackage with more
  than one table) currently print the **first** table's schema.
  Cross-table schema listing isn't exposed via the CLI yet; the
  MCP server's [`list_tables`](../mcp/tools/list_tables.md) tool
  covers that case.
- **Streaming formats** (Parquet, CSV, TSV) load the standard
  initial-row batch (5 Million rows by default, override with
  `--rows N|all`) and then project the schema out, so the cost is
  the read cost of the cap, not the whole file. For schema-only
  inspection on multi-GB files, this is usually still sub-second on
  Parquet. Parquet files with very many row groups fall back to a
  DuckDB-backed reader automatically.
- **Text formats** (CSV, JSON, etc.) infer types from the header
  row, following the same rules the GUI uses.
- **Read-only formats** are supported just fine; schema works for
  SAS, RDS, HDF5, NetCDF, EPUB, GeoJSON the same as for Parquet.

## See also

- [`octa --head`](head.md) prints the first N rows alongside the
  schema.
- [MCP `schema` tool](../mcp/tools/schema.md) is the same behaviour
  via the MCP server.
