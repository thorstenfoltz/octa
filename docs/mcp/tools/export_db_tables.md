# `export_db_tables`

Export the tables of one schema of a saved live-database connection into a
folder, one file per table, named after the table.

Each table is read from the server page by page and written as the pages
arrive, so its size does not matter: a table with tens of millions of rows
never has to fit in memory. A file only appears under its real name once it is
complete. One table that fails does not stop the others.

**Write tool.** It writes files, so it is removed from the roster entirely
when the server runs with `--mcp-read-only`. In the in-app Assistant the
folder follows the same rules as every other write: inside the export
directory unless write protection is off.

## Input schema

| Name         | Type              | Meaning                                                           |
|--------------|-------------------|-------------------------------------------------------------------|
| `connection` | string (required) | Connection to read from (name or id)                              |
| `schema`     | string (required) | Schema whose tables are exported (the database name on MySQL)     |
| `out_dir`    | string (required) | Folder the files go into; created if absent                       |
| `tables`     | array of strings  | Tables to export; default every table of the schema               |
| `format`     | string            | `parquet` (default), `csv`, `tsv` or `jsonl`                      |
| `overwrite`  | boolean           | Replace files that are already there; default `false` (skip them) |
| `catalog`    | string            | Catalog on Snowflake, Databricks or BigQuery                      |

CSV and TSV take their quoting, line endings, decimal mark and byte order mark
from **Settings > Files > Write options**, like every other write.

## Result

```json
{
  "written": 2, "skipped": 1, "failed": 0, "rows": 1284003,
  "items": [
    { "table": "orders", "status": "written", "path": "dump/orders.parquet", "rows": 1284000 },
    { "table": "regions", "status": "written", "path": "dump/regions.parquet", "rows": 3 },
    { "table": "customers", "status": "skipped", "path": "dump/customers.parquet" }
  ]
}
```

A failed table has `"status": "failed"` and an `error` instead of a path.

## See also

- [`copy_db_table`](copy_db_table.md) to copy a table to another server instead.
- In the app: [Exporting tables to files](../../usage/working-with-databases.md#exporting-tables-to-files)
- Command line: [`--db-export`](../../cli/databases.md#exporting-tables-to-files)
