# Tools Reference

The MCP server exposes eleven tools. All are **read-only** against a
file argument except `convert`, which writes a new file but doesn't
modify the input.

## At-a-glance

| Tool                                        | Purpose                                   | Mutates files?                  |
|---------------------------------------------|-------------------------------------------|---------------------------------|
| **[`read_table`](read_table.md)**           | Load schema + rows from a file            | No                              |
| **[`schema`](schema.md)**                   | Schema only (no rows)                     | No                              |
| **[`list_tables`](list_tables.md)**         | List tables in a multi-table source       | No                              |
| **[`count_rows`](count_rows.md)**           | Row count for a table                     | No                              |
| **[`run_sql`](run_sql.md)**                 | DuckDB SQL against the file               | No *                            |
| **[`convert`](convert.md)**                 | Write a file in a different format        | Writes only the new output path |
| **[`export_schema`](export_schema.md)**     | Render the schema as DDL / model / struct | No                              |
| **[`profile`](profile.md)**                 | Per-column statistics (`SUMMARIZE`)       | No                              |
| **[`find_duplicates`](find_duplicates.md)** | Rows sharing key-column values            | No                              |
| **[`value_frequency`](value_frequency.md)** | Per-column value counts                   | No                              |
| **[`search`](search.md)**                   | Match cells across every column           | No                              |

\* `run_sql` accepts mutation queries (`INSERT` / `UPDATE` / `DELETE`)
but the in-memory DuckDB connection is discarded at the end of the
call. Changes are not persisted back to the file, and the next tool
call sees the original on-disk contents again. The mutation result
is only useful for "what would this query produce?" probes.

## Common parameters

All tools share two parameter conventions:

- `path` is required. Absolute or working-directory-relative
  path to the file. Octa parses based on the file extension.
- `table` *(optional)*: for multi-table sources (SQLite,
  DuckDB, GeoPackage), pick a specific table. Omit for
  single-table formats. If you don't know the available tables,
  call [`list_tables`](list_tables.md) first.

Row-returning tools (`read_table`, `run_sql`, `find_duplicates`,
`search`) also share:

- `limit` *(optional)*: maximum rows / hits to return.
  - Omit → use the server's configured default (1000 unless changed
      under **Settings → MCP**).
  - `0` → unlimited (returns every row, so be careful with big
      files).
  - Any positive integer → that many rows max.

## Response shape

Tools return JSON content. The shape varies by tool (see each tool
page for the specifics), but result-bearing tools always include
these envelope fields:

| Field                  | Type | Meaning                                                                                              |
|------------------------|------|------------------------------------------------------------------------------------------------------|
| `truncated`            | bool | True when more rows existed than were returned                                                       |
| `total_rows_available` | int  | Total rows in the source (when known cheaply)                                                        |
| `cell_truncated`       | bool | True when at least one cell was replaced with a `[truncated: …]` marker due to the per-cell byte cap |

These flags let an AI client know when to ask for more, e.g. if
`truncated: true` and `total_rows_available: 50000`, the model can
re-call with `limit: 0` (or a higher limit) when the user asks for
"all of them."

## Error handling

Errors come back as MCP `tool error` responses with a message and
an error code:

| Code             | Meaning                                                         |
|------------------|-----------------------------------------------------------------|
| `invalid_params` | The arguments couldn't be parsed or the file couldn't be opened |
| `internal_error` | Unexpected failure inside the tool's logic (rare)               |

Friendly examples:

```json
{ "error": { "code": "invalid_params", "message": "read failed: no reader available for /tmp/data.unknown" }}
{ "error": { "code": "invalid_params", "message": "run_sql failed: syntax error at \"FOO\"" }}
{ "error": { "code": "invalid_params", "message": "convert failed: format SAS does not support writing" }}
```

The model sees the error and (in practice) usually responds with a
clarifying question or corrected call.

## See also

- Each tool page for input schema + worked examples.
- [Limits & truncation](../limits-and-truncation.md) for how
  `truncated` and `cell_truncated` are computed.
- [Examples](../examples.md) for end-to-end prompts that exercise
  the tools.
