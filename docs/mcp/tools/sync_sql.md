# `sync_sql`

Return the SQL that would make a live database table match a file, without
running any of it. Read-only, so it stays available under `--mcp-read-only`.

This is the tool for "show me the change before it happens". The script is
rendered by the same code the GUI's **File > Save SQL** and the CLI's
`--sync-sql` use, so a script reviewed here and a write-back applied there
cannot drift apart.

## Parameters

| Name         | Type     | Meaning                                                    |
|--------------|----------|------------------------------------------------------------|
| `path`       | string   | Source file holding the desired state. May be a cloud URL. |
| `open_tab`   | string   | Use an open GUI tab instead (name, `@active`, or `#2`).    |
| `table`      | string   | Sheet or table name for a multi-table source file.         |
| `connection` | string   | Saved connection name or id from **Settings > Databases**. |
| `target`     | string   | Target table as `SCHEMA.TABLE` or `CATALOG.SCHEMA.TABLE`.  |
| `on`         | string[] | Key columns matching source rows to server rows. Required. |

`on` is required and cannot be empty: without a key there is no way to tell an
updated row from a delete plus an insert. Call
[`list_db_connections`](../index.md) first if you do not know the connection
names.

## Response

```json
{
  "connection": "prod",
  "target": "public.users",
  "sql": "BEGIN;\nUPDATE public.users SET ...;\nINSERT INTO public.users ...;\nCOMMIT;",
  "deletes": 0,
  "updates": 2,
  "inserts": 1,
  "ignored_columns": ["imported_at"]
}
```

The script is one transaction, ordered the way a real write-back applies it:
added columns, deletes, full-row updates, inserts.

## Notes

- **Nothing is written.** The tool reads the server table and returns text.
- Numbers are compared as numbers, so a file's `120.50` and a `numeric(12,2)`
  column's `120.50` do not produce a phantom `UPDATE`.
- Columns present only in the source are listed in `ignored_columns` and
  skipped. This never emits `ALTER TABLE`.
- Only the rows the source file holds are compared, so a partially loaded
  source produces a partial script.

See [Database Connections](../../usage/database-connections.md) for the GUI and
CLI forms of the same question.
