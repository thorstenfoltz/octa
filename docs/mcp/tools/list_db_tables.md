# `list_db_tables`

List the schemas and tables of a saved live-database connection.

Use it between [`list_db_connections`](list_db_connections.md) and
[`query_db`](query_db.md): it tells you what you are allowed to write in
a `FROM` clause, so a query does not have to guess a table name.

Read-only, and kept when the server runs with `--mcp-read-only`.

## Parameters

| Name         | Type              | Meaning                                                                                 |
|--------------|-------------------|-----------------------------------------------------------------------------------------|
| `connection` | string (required) | Saved connection name or id                                                             |
| `schema`     | string            | List only this schema's tables. Absent: every schema                                    |
| `catalog`    | string            | Top namespace level on Snowflake, Databricks and BigQuery. An error on any other engine |

## Result

```json
{
  "connection": "prod",
  "engine": "PostgreSQL",
  "schemas": ["public", "reporting"],
  "tables": [
    { "schema": "public", "table": "customers" },
    { "schema": "public", "table": "orders" },
    { "schema": "reporting", "table": "daily_sales" }
  ]
}
```

## Three levels, not two

Snowflake, Databricks and BigQuery put a **catalog** above the schema.
On those engines, calling this without `catalog` lists the catalogs
themselves; pass one to descend into its schemas and tables. Every other
engine has two levels and rejects `catalog` as an error rather than
ignoring it.

The names map to each vendor's own vocabulary: a Databricks catalog, a
Snowflake database, a BigQuery project.

## See also

- [`query_db`](query_db.md) — read from a table you found here.
- [`db_relationships`](db_relationships.md) — how those tables connect.
- [Database Connections](../../usage/database-connections.md).
