# Tools Reference

The MCP server exposes the tools below. Most are **read-only** against a
file argument. The file-writing exceptions are `convert` (writes a new
output file), `write_table` (writes model-supplied rows to a new file),
`edit_table` (edits an existing file in place), `transform_columns`
(rename / cast / drop columns, writes back), `anonymize` (mask /
scramble columns, writes the result), and `partition_table` (writes one
file per group). These are dropped when the server is started with
`--mcp-read-only`.

## At-a-glance

| Tool                                                        | Purpose                                                 | Mutates files?                  |
|-------------------------------------------------------------|---------------------------------------------------------|---------------------------------|
| **[`read_table`](read_table.md)**                           | Load schema + rows from a file                          | No                              |
| **[`tail`](tail.md)**                                       | Last N rows of a file                                   | No                              |
| **[`sample`](sample.md)**                                   | Reproducible random N-row sample                        | No                              |
| **[`schema`](schema.md)**                                   | Schema only (no rows)                                   | No                              |
| **[`list_tables`](list_tables.md)**                         | List tables in a multi-table source                     | No                              |
| **[`count_rows`](count_rows.md)**                           | Row count for a table                                   | No                              |
| **[`run_sql`](run_sql.md)**                                 | DuckDB SQL against the file                             | No *                            |
| **[`convert`](convert.md)**                                 | Write a file in a different format                      | Writes only the new output path |
| **[`export_schema`](export_schema.md)**                     | Render the schema as DDL / model / struct               | No                              |
| **[`profile`](profile.md)**                                 | Per-column statistics (`SUMMARIZE`)                     | No                              |
| **[`find_duplicates`](find_duplicates.md)**                 | Rows sharing key-column values                          | No                              |
| **[`fuzzy_duplicates`](fuzzy_duplicates.md)**               | Near-duplicate row clusters (fuzzy)                     | No                              |
| **[`value_frequency`](value_frequency.md)**                 | Per-column value counts                                 | No                              |
| **[`search`](search.md)**                                   | Match cells across every column                         | No                              |
| **[`compare_schemas`](compare_schemas.md)**                 | Diff the column metadata of two files                   | No                              |
| **[`diff_tables`](diff_tables.md)**                         | Row-level diff of two files                             | No                              |
| **[`describe_file`](describe_file.md)**                     | One-shot orientation snapshot                           | No                              |
| **[`validate_against_schema`](validate_against_schema.md)** | Validate columns against a JSON Schema                  | No                              |
| **[`unique_columns`](unique_columns.md)**                   | Unique columns / key candidates                         | No                              |
| **[`pivot`](pivot.md)**                                     | Reshape long <-> wide (PIVOT / UNPIVOT)                 | No                              |
| **[`correlation`](correlation.md)**                         | Pairwise numeric correlation matrix                     | No                              |
| **[`grep_files`](grep_files.md)**                           | Grep a value across files in a directory                | No                              |
| **[`list_objects`](list_objects.md)**                       | List a cloud bucket folder (S3/Azure/GCS)               | No                              |
| **[`write_table`](write_table.md)**                         | Write inline rows to a new file                         | Writes/replaces the output path |
| **[`edit_table`](edit_table.md)**                           | Add columns / set cells / insert / delete rows in place | Yes (edits the file)            |
| **[`transform_columns`](transform_columns.md)**             | Rename / cast / drop columns, write back                | Writes the output path          |
| **[`anonymize`](anonymize.md)**                             | Mask / scramble columns, write the result               | Writes the output path          |
| **[`detect_pii`](detect_pii.md)**                           | Find likely personal-data columns                       | No                              |
| **[`detect_outliers`](detect_outliers.md)**                 | Flag numeric outlier cells                              | No                              |
| **[`fill_missing`](fill_missing.md)**                       | Impute empty cells in a column                          | No                              |
| **[`drop_duplicates`](drop_duplicates.md)**                 | Remove duplicate rows                                   | No                              |
| **[`union_tables`](union_tables.md)**                       | Stack tables vertically                                 | No                              |
| **[`join_tables`](join_tables.md)**                         | Join tables on key columns                              | No                              |
| **[`partition_table`](partition_table.md)**                 | One file per distinct column value                      | Writes one file per group       |

\* `run_sql` accepts mutation queries (`INSERT` / `UPDATE` / `DELETE`)
but the in-memory DuckDB connection is discarded at the end of the
call. Changes are not persisted back to the file, and the next tool
call sees the original on-disk contents again. The mutation result
is only useful for "what would this query produce?" probes.

## Common parameters

All tools share two parameter conventions:

- `path` is required. Absolute or working-directory-relative
  path to the file. Octa parses based on the file extension.
  A **cloud URL** (`s3://bucket/key`, `az://container/key`, `gs://bucket/key`)
  is also accepted: the object is downloaded to a temporary file and read as
  usual. The MCP/CLI server authenticates with **ambient credentials** (AWS_*
  env vars, a cached SSO session, Azure CLI login, or Google
  application-default credentials); Azure also needs `AZURE_STORAGE_ACCOUNT`.
  Use [`list_objects`](list_objects.md) to browse a bucket first.
  **Writing** to a cloud URL works too: the write tools (`write_table`,
  `convert`, `transform_columns`, `anonymize`, `run_sql` with `write_to`)
  accept a cloud URL as their output, building the file locally and uploading
  it. They use the same ambient credentials; run the server with
  `--mcp-read-only` to drop all write tools.
- `table` *(optional)*: for multi-table sources (SQLite,
  DuckDB, GeoPackage), pick a specific table. Omit for
  single-table formats. If you don't know the available tables,
  call [`list_tables`](list_tables.md) first.

Row-returning tools (`read_table`, `tail`, `sample`, `run_sql`,
`find_duplicates`, `search`, `diff_tables`) also share:

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
