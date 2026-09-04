# Tools Reference

The MCP server exposes the tools below. Most are **read-only** against a
file argument. The file-writing exceptions are `convert` (writes a new
output file), `write_table` (writes model-supplied rows to a new file),
`edit_table` (edits an existing file in place), `transform_columns`
(rename / cast / drop columns, writes back), `anonymize` (mask /
scramble columns, writes the result), `partition_table` (writes one
file per group), and `write_workbook` (writes one `.xlsx` holding several
tables). The live-database write tools (`write_db_table`,
`copy_db_table`) and `run_sql`'s `write_to` are gated the same way. All
of these are dropped when the server is started with `--mcp-read-only`.

## Which tools reach the model, and when

All of the tools below exist in every build. What differs is how many of them
are described to a model at any one time, because those descriptions are not
free: together they are about **33,000 tokens** of text and JSON schema, and a
model has to be told about a tool before it can call one.

There are three separate switches, for three different jobs:

| Switch                                                                               | Where              | What it is for                                                                                                                    |
|--------------------------------------------------------------------------------------|--------------------|-----------------------------------------------------------------------------------------------------------------------------------|
| [`--mcp-read-only`](../setup.md#read-only-mode)                                      | MCP flag           | **Safety.** Nothing can change a file, a tab or a database. Wins over everything else.                                            |
| [`--mcp-tools` / `--mcp-without`](../setup.md#advertising-fewer-tools)               | MCP flag           | **Context size.** The client reads the tool list once and carries it in every request to its model, so a smaller list is cheaper. |
| [Assistant tools](../../usage/chatbot.md#choosing-which-tools-the-assistant-may-use) | Settings, GUI only | **Preference.** Which tools Octa's own assistant may use at all, on this machine.                                                 |

Why an MCP flag rather than a setting: an MCP server is launched by its client,
from that client's own config file, and different clients want different
surfaces from the same Octa install. A flag lives in exactly that config; a
global setting would apply to all of them at once.

Why the assistant gets an extra trick: Octa builds the assistant's requests
itself, so it can send the `core` group up front and **load a group during the
conversation** when a job needs it. Over MCP the client owns the tool list and
decides what reaches its model, so a static filter is the only lever that
works. See
[Token count](../../usage/chatbot.md#token-count) for how the assistant side
behaves.

The groups, with roughly what each costs a request:

| Group       | Tools | Approx. tokens | For                                                    |
|-------------|-------|----------------|--------------------------------------------------------|
| `core`      | 13    | ~6,000         | reading, schemas, counting, search, profiling, SQL     |
| `quality`   | 10    | ~5,000         | duplicates, outliers, PII, rules, correlations         |
| `compare`   | 5     | ~2,900         | schema and row differences, drift                      |
| `combine`   | 6     | ~4,000         | unions, joins, join keys, reconciling column names     |
| `reshape`   | 8     | ~6,000         | pivot, resample, rolling windows, dedupe, impute, mask |
| `databases` | 7     | ~3,000         | live database connections, their tables and queries    |
| `cloud`     | 4     | ~1,300         | objects in S3, Azure and GCS                           |
| `write`     | 9     | ~5,100         | writing files, converting, charts and reports          |

Those totals are measured from the descriptions and schemas themselves, so they
move a little as tools are documented. The **Settings > Chat / Assistant >
Assistant tools** list shows the current number per tool. (`core` and `write`
count 13 and 9 above, rather than the 12 and 6 in the table below, because four
tools exist only inside the GUI assistant and have no MCP equivalent:
`read_text`, `write_text`, `edit_open_tab` and `create_chart`.)

## At-a-glance

| Tool                                                        | Group       | Purpose                                                 | Mutates files?                  |
|-------------------------------------------------------------|-------------|---------------------------------------------------------|---------------------------------|
| **[`read_table`](read_table.md)**                           | `core`      | Load schema + rows from a file                          | No                              |
| **[`tail`](tail.md)**                                       | `core`      | Last N rows of a file                                   | No                              |
| **[`sample`](sample.md)**                                   | `core`      | Reproducible random N-row sample                        | No                              |
| **[`schema`](schema.md)**                                   | `core`      | Schema only (no rows)                                   | No                              |
| **[`list_tables`](list_tables.md)**                         | `core`      | List tables in a multi-table source                     | No                              |
| **[`count_rows`](count_rows.md)**                           | `core`      | Row count for a table                                   | No                              |
| **[`run_sql`](run_sql.md)**                                 | `core`      | DuckDB SQL against the file                             | No *                            |
| **[`convert`](convert.md)**                                 | `write`     | Write a file in a different format                      | Writes only the new output path |
| **[`export_schema`](export_schema.md)**                     | `compare`   | Render the schema as DDL / model / struct               | No                              |
| **[`profile`](profile.md)**                                 | `core`      | Per-column statistics (`SUMMARIZE`)                     | No                              |
| **[`find_duplicates`](find_duplicates.md)**                 | `quality`   | Rows sharing key-column values                          | No                              |
| **[`fuzzy_duplicates`](fuzzy_duplicates.md)**               | `quality`   | Near-duplicate row clusters (fuzzy)                     | No                              |
| **[`value_frequency`](value_frequency.md)**                 | `core`      | Per-column value counts                                 | No                              |
| **[`search`](search.md)**                                   | `core`      | Match cells across every column                         | No                              |
| **[`compare_schemas`](compare_schemas.md)**                 | `compare`   | Diff the column metadata of two files                   | No                              |
| **[`diff_tables`](diff_tables.md)**                         | `compare`   | Row-level diff of two files                             | No                              |
| **[`describe_file`](describe_file.md)**                     | `core`      | One-shot orientation snapshot                           | No                              |
| **[`validate_against_schema`](validate_against_schema.md)** | `quality`   | Validate columns against a JSON Schema                  | No                              |
| **[`unique_columns`](unique_columns.md)**                   | `quality`   | Unique columns / key candidates                         | No                              |
| **[`suggest_join_keys`](suggest_join_keys.md)**             | `combine`   | Rank the column pairs that would join two tables        | No                              |
| **[`pivot`](pivot.md)**                                     | `reshape`   | Reshape long <-> wide (PIVOT / UNPIVOT)                 | No                              |
| **[`batch_convert`](batch_convert.md)**                     | `write`     | Convert many files into one format                      | Yes                             |
| **[`resample_timeseries`](resample_timeseries.md)**         | `reshape`   | Group rows into time buckets and aggregate              | No                              |
| **[`rolling_window`](rolling_window.md)**                   | `reshape`   | Rolling aggregate over the previous N rows              | No                              |
| **[`correlation`](correlation.md)**                         | `quality`   | Pairwise numeric correlation matrix                     | No                              |
| **[`compare_distributions`](compare_distributions.md)**     | `quality`   | Whether two columns are shaped alike                    | No                              |
| **[`grep_files`](grep_files.md)**                           | `core`      | Grep a value across files in a directory                | No                              |
| **[`list_objects`](list_objects.md)**                       | `cloud`     | List a cloud bucket folder (S3/Azure/GCS)               | No                              |
| **[`copy_object`](copy_object.md)**                         | `cloud`     | Copy a cloud object or folder to another location       | Yes                             |
| **[`move_object`](move_object.md)**                         | `cloud`     | Move a cloud object or folder (copy, then delete)       | Yes                             |
| **[`delete_object`](delete_object.md)**                     | `cloud`     | Delete a cloud object or folder                         | Yes                             |
| **[`write_table`](write_table.md)**                         | `write`     | Write inline rows to a new file                         | Writes/replaces the output path |
| **[`edit_table`](edit_table.md)**                           | `write`     | Add columns / set cells / insert / delete rows in place | Yes (edits the file)            |
| **[`transform_columns`](transform_columns.md)**             | `reshape`   | Rename / cast / drop columns, write back                | Writes the output path          |
| **[`anonymize`](anonymize.md)**                             | `reshape`   | Mask / scramble columns, write the result               | Writes the output path          |
| **[`detect_pii`](detect_pii.md)**                           | `quality`   | Find likely personal-data columns                       | No                              |
| **[`detect_outliers`](detect_outliers.md)**                 | `quality`   | Flag numeric outlier cells                              | No                              |
| **[`fill_missing`](fill_missing.md)**                       | `reshape`   | Impute empty cells in a column                          | No                              |
| **[`drop_duplicates`](drop_duplicates.md)**                 | `reshape`   | Remove duplicate rows                                   | No                              |
| **[`union_tables`](union_tables.md)**                       | `combine`   | Stack tables vertically                                 | No                              |
| **[`join_tables`](join_tables.md)**                         | `combine`   | Join tables on key columns                              | No                              |
| **[`partition_table`](partition_table.md)**                 | `reshape`   | One file per distinct column value                      | Writes one file per group       |
| **[`schema_drift`](schema_drift.md)**                       | `compare`   | Which files in a folder disagree about columns          | No                              |
| **[`harmonise_schemas`](harmonise_schemas.md)**             | `combine`   | Rewrite a folder of files to one schema                 | Writes into out_dir             |
| **[`create_report`](create_report.md)**                     | `write`     | Write a self-contained HTML profiling report            | Writes the output path          |
| **[`fuzzy_join`](fuzzy_join.md)**                           | `combine`   | Join on similarity rather than equality                 | No                              |
| **[`diagnose_join`](diagnose_join.md)**                     | `combine`   | Why two key columns do not join                         | No                              |
| **[`check_rules`](check_rules.md)**                         | `quality`   | Check values against a saved rules file                 | No                              |
| **[`check_references`](check_references.md)**               | `quality`   | Whether keys in one table exist in another              | No                              |
| **[`data_drift`](data_drift.md)**                           | `compare`   | How a dataset changed between two versions              | No                              |
| **[`sync_sql`](sync_sql.md)**                               | `databases` | SQL that would make a server table match a file         | No                              |
| **[`write_workbook`](write_workbook.md)**                   | `write`     | Write several tables into one .xlsx workbook            | Writes the output path          |
| **[`list_db_connections`](list_db_connections.md)** [^db]   | `databases` | List saved live-database connections                    | No                              |
| **[`list_db_tables`](list_db_tables.md)** [^db]             | `databases` | List schemas / tables on a live connection              | No                              |
| **[`db_relationships`](db_relationships.md)** [^db]         | `databases` | Foreign keys a live database declares                   | No                              |
| **[`query_db`](query_db.md)** [^db]                         | `databases` | Run SQL on a live database server                       | Mutations need Allow writes     |
| **[`write_db_table`](write_db_table.md)** [^db]             | `databases` | Write a table into a live database                      | Yes (server table)              |
| **[`copy_db_table`](copy_db_table.md)** [^db]               | `databases` | Copy a table server-to-server through DuckDB            | Yes (target server table)       |

[^db]: The live-database tools work on the connections saved under
    **Settings -> Databases** (loaded once at server startup) and are
    described in [Database Connections](../../usage/database-connections.md).
    Every write is additionally gated on the connection's own
    **Allow writes** switch. On Snowflake, Databricks and BigQuery, which
    have a catalog level above the schema, `list_db_tables` and
    `write_db_table` take a `catalog` parameter and `copy_db_table` takes
    `source_catalog` and `target_catalog`; `list_db_tables` without a
    catalog returns the catalog list itself.

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
