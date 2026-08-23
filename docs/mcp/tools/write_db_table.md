# `write_db_table`

Write a table into a saved live-database connection.

The source is a file or an open tab; the target is a schema and table on
the server. Use it to land a cleaned or converted result somewhere other
tools can reach.

**Write tool.** Removed from the roster entirely when the server runs
with `--mcp-read-only`.

## Parameters

| Name         | Type              | Meaning                                                                            |
|--------------|-------------------|------------------------------------------------------------------------------------|
| `connection` | string (required) | Saved connection name or id                                                        |
| `schema`     | string (required) | Target schema (`public`, `dbo`, or the database name on MySQL)                     |
| `table`      | string (required) | Target table name                                                                  |
| `mode`       | string            | `create` (default, error if the table exists), `append`, `replace` (DROP + CREATE) |
| `path`       | string            | Source file, any supported format. Cloud URLs included                             |
| `open_tab`   | string            | Source open tab (in-GUI assistant only): `@active`, a tab name, or `#2`            |
| `catalog`    | string            | Top namespace level on Snowflake, Databricks and BigQuery. An error elsewhere      |

Give exactly one of `path` or `open_tab`.

## Result

```json
{ "rows_written": 4821, "created": true }
```

## The write is gated

Refused unless the connection's **Allow writes** switch is on. That
switch is off by default and is per connection, so a read-only reporting
connection cannot be written to by accident — check it first with
[`list_db_connections`](list_db_connections.md).

`replace` drops the existing table before recreating it. There is no
undo on a live server: prefer `create` when the table should not already
exist, and let the error tell you when it does.

## Types

Columns are created from the source's Arrow types through the same
dialect mapping [`export_schema`](export_schema.md) uses, so the DDL
matches what that tool would have rendered for the engine. Identifiers
are quoted in the `CREATE` and the `INSERT` identically, so a column with
a capital letter or a space survives the round trip.

## See also

- [`copy_db_table`](copy_db_table.md) — server to server, no file in the middle.
- [`sync_sql`](sync_sql.md) — get the SQL for review instead of writing.
- [Database Connections](../../usage/database-connections.md).
