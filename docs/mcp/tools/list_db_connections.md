# `list_db_connections`

List the live-database connections saved under **Settings → Databases**.

This is the starting point for every other database tool: they all take a
`connection` argument, and this is where the names come from. It reads
Octa's own settings and **contacts no server**, so it is safe to call
first and costs nothing.

Read-only, and kept when the server runs with `--mcp-read-only`.

## Parameters

None.

## Result

```json
{
  "count": 2,
  "connections": [
    {
      "name": "prod",
      "engine": "PostgreSQL",
      "host": "db.example.com",
      "port": 5432,
      "database": "shop",
      "allow_writes": false
    },
    {
      "name": "warehouse",
      "engine": "Snowflake",
      "host": "acme-eu",
      "port": 443,
      "database": "ANALYTICS",
      "allow_writes": true
    }
  ]
}
```

## Read `allow_writes` before planning a write

`allow_writes` is the per-connection switch in Settings, **off by
default**. When it is false, [`query_db`](query_db.md) refuses mutations,
[`write_db_table`](write_db_table.md) refuses outright, and
[`copy_db_table`](copy_db_table.md) refuses to use it as a target.
Checking here first turns a failed write into a question you can ask
before starting.

The server's own `--mcp-read-only` flag is separate and stricter: it
removes the write tools from the roster entirely, whatever a connection
allows.

## Secrets

Passwords, tokens and keys are **never** returned. They live in the OS
keyring (or, with `OCTA_NO_KEYRING=1`, in `settings.toml`) and are read
only at connect time.

## See also

- [Database Connections](../../usage/database-connections.md) — setting
  them up, and what each engine needs.
- [`list_db_tables`](list_db_tables.md) — what is inside one.
