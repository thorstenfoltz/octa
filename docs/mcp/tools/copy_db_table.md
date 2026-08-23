# `copy_db_table`

Copy a table from one saved live-database connection to another,
streamed **server to server**.

The rows never pass through a file or through Octa's row caps: an
in-memory DuckDB attaches both servers and moves the data directly, using
binary `COPY` on the PostgreSQL side. That makes it the right tool for a
large table, where [`write_db_table`](write_db_table.md) would have to
read the source into memory first.

**Write tool.** Removed from the roster entirely when the server runs
with `--mcp-read-only`.

## Parameters

| Name                | Type              | Meaning                                                                                |
|---------------------|-------------------|----------------------------------------------------------------------------------------|
| `source_connection` | string (required) | Connection to read from                                                                |
| `source_schema`     | string (required) | Source schema (the database name on MySQL)                                             |
| `source_table`      | string (required) | Source table name                                                                      |
| `target_connection` | string (required) | Connection to write to                                                                 |
| `target_schema`     | string            | Defaults to `public` on PostgreSQL / Redshift, the connection's own database otherwise |
| `target_table`      | string            | Defaults to the source table name                                                      |
| `mode`              | string            | `create` (default, error if the target exists), `append`, `replace` (DROP + CREATE)    |
| `source_catalog`    | string            | Source catalog on Snowflake, Databricks or BigQuery                                    |
| `target_catalog`    | string            | Target catalog on Snowflake, Databricks or BigQuery                                    |

## Result

```json
{ "rows_copied": 1284003, "created": true }
```

## PostgreSQL and MySQL only

The fast lane needs DuckDB's native `postgres` and `mysql` extensions on
both ends, so those are the engines this supports, in either direction.
SQL Server is refused with a pointer to
[`write_db_table`](write_db_table.md), which reaches every engine by
reading the source and writing rows.

The extensions install over the network the first time they are used,
then stay cached.

## Refusals

- The target connection's **Allow writes** switch must be on.
- Copying a table onto itself (same connection, same schema, same table)
  is refused rather than attempted.

## See also

- [`write_db_table`](write_db_table.md) — any engine, from a file or tab.
- [Database Connections](../../usage/database-connections.md).
