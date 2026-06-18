# MCP Server

Octa includes a built-in **MCP (Model Context Protocol) server**.
Run `octa --mcp` and Octa speaks JSON-RPC over stdin/stdout,
exposing a set of tools that let an MCP-aware client (Claude Desktop,
Claude Code, MCP Inspector, etc.) interact with your local data
files.

The MCP server is the most popular way to wire Octa into AI
workflows: instead of describing your data file to Claude in words,
let Claude open it, run SQL, count rows, and convert formats on its
own.

## Why MCP?

[Model Context Protocol](https://modelcontextprotocol.io/) is an
open standard for connecting AI models to external tools and data
sources. An MCP server runs locally on your machine, exposes a set
of typed tools, and an MCP-aware client (the AI app) calls those
tools on the model's behalf.

Octa's MCP server is **fully local**: the AI client connects to
Octa via stdio (the AI process spawns `octa --mcp` as a subprocess),
and no network calls leave your machine for the data operations
themselves. Your files stay on disk.

## The tools

| Tool                      | What it does                                  | Reference                                 |
|---------------------------|-----------------------------------------------|-------------------------------------------|
| `read_table`              | Load a file and return schema + rows as JSON  | [→ doc](tools/read_table.md)              |
| `tail`                    | Return the last N rows of a file              | [→ doc](tools/tail.md)                    |
| `sample`                  | Reproducible random N-row sample              | [→ doc](tools/sample.md)                  |
| `schema`                  | Return column schema only (no rows)           | [→ doc](tools/schema.md)                  |
| `list_tables`             | List tables in a multi-table source           | [→ doc](tools/list_tables.md)             |
| `count_rows`              | Count rows in a tabular file                  | [→ doc](tools/count_rows.md)              |
| `run_sql`                 | Run a DuckDB SQL query against a file         | [→ doc](tools/run_sql.md)                 |
| `convert`                 | Convert a file from one format to another     | [→ doc](tools/convert.md)                 |
| `export_schema`           | Render the schema as DDL / a model / a struct | [→ doc](tools/export_schema.md)           |
| `profile`                 | Per-column statistics via `SUMMARIZE`         | [→ doc](tools/profile.md)                 |
| `find_duplicates`         | Find rows sharing key-column values           | [→ doc](tools/find_duplicates.md)         |
| `fuzzy_duplicates`        | Cluster near-duplicate rows (fuzzy)           | [→ doc](tools/fuzzy_duplicates.md)        |
| `value_frequency`         | Count per-column values (`value_counts`)      | [→ doc](tools/value_frequency.md)         |
| `search`                  | Match cells across every column               | [→ doc](tools/search.md)                  |
| `compare_schemas`         | Diff the column metadata of two files         | [→ doc](tools/compare_schemas.md)         |
| `diff_tables`             | Row-level diff of two files                   | [→ doc](tools/diff_tables.md)             |
| `describe_file`           | One-shot orientation snapshot                 | [→ doc](tools/describe_file.md)           |
| `validate_against_schema` | Validate columns against a JSON Schema        | [→ doc](tools/validate_against_schema.md) |
| `unique_columns`          | Unique columns / key candidates               | [→ doc](tools/unique_columns.md)          |
| `pivot`                   | Reshape long <-> wide (PIVOT / UNPIVOT)       | [→ doc](tools/pivot.md)                   |
| `correlation`             | Pairwise numeric correlation matrix           | [→ doc](tools/correlation.md)             |
| `grep_files`              | Grep a value across files in a directory      | [→ doc](tools/grep_files.md)              |
| `write_table`             | Write inline rows to a new file               | [→ doc](tools/write_table.md)             |
| `edit_table`              | Set cells / insert / delete rows in place     | [→ doc](tools/edit_table.md)              |
| `transform_columns`       | Rename / cast / drop columns, write back      | [→ doc](tools/transform_columns.md)       |
| `anonymize`               | Mask / scramble columns, write the result     | [→ doc](tools/anonymize.md)               |
| `detect_pii`              | Find likely personal-data columns             | [→ doc](tools/detect_pii.md)              |
| `detect_outliers`         | Flag numeric outlier cells                    | [→ doc](tools/detect_outliers.md)         |
| `fill_missing`            | Impute empty cells in a column                | [→ doc](tools/fill_missing.md)            |
| `drop_duplicates`         | Remove duplicate rows                         | [→ doc](tools/drop_duplicates.md)         |
| `union_tables`            | Stack tables vertically                       | [→ doc](tools/union_tables.md)            |
| `join_tables`             | Join tables on key columns                    | [→ doc](tools/join_tables.md)             |
| `partition_table`         | One file per distinct column value            | [→ doc](tools/partition_table.md)         |

Every tool that returns rows respects a configurable response
row limit (default 1000) and cell byte cap (default 64 KiB),
so Claude doesn't accidentally pull a 100 GB file's worth of bytes
through the JSON-RPC channel. Streaming formats (Parquet, CSV, TSV)
additionally honour a file-loader cap (default 5,000,000 rows).
Per-call, `limit: 0` lifts the response cap and `unlimited: true`
lifts the file-loader cap. Parquet files
with very many row groups fall back to a DuckDB-backed reader
automatically. See [Limits & Truncation](limits-and-truncation.md)
for the full mechanics.

## What this gets you

A few real-world prompts that "just work" once Octa is wired into
Claude:

> **You:** What columns does `~/data/sales-q4.parquet` have?
>
> **Claude:** *(calls `schema`)* region (Utf8), quarter (Utf8),
> amount (Float64), order_id (Int64).

> **You:** How many rows are in `users.sqlite`?
>
> **Claude:** *(calls `list_tables` to find the table names, then
> `count_rows` on each)* three tables: users (1,247,832 rows),
> orders (4,891,002 rows), products (12,408 rows).

> **You:** What was the average order value last quarter?
>
> **Claude:** *(calls `run_sql` with `SELECT AVG(amount) FROM data
> WHERE quarter = 'Q4'`)* $187.42 across 423,019 orders.

> **You:** Convert `messy.xlsx` to a clean Parquet file.
>
> **Claude:** *(calls `convert` with input + output paths)* wrote
> 14,523 rows × 8 columns to `messy.parquet`.

> **You:** Give me a quick profile of `events.parquet`.
>
> **Claude:** *(calls `profile`)* 6 columns: `user_id` (BIGINT, 0 %
> null, 8.4 k distinct), `amount` (DOUBLE, min 0.0 / max 998.5 / mean
> 41.2), `country` (VARCHAR, 3 % null, 47 distinct)…

> **You:** Generate a Snowflake `CREATE TABLE` for `sales.parquet`.
>
> **Claude:** *(calls `export_schema` with `target: snowflake`)*
> here's the DDL: `CREATE TABLE "sales" ( … )`.

## How it fits together

```
┌───────────────────────────────────────────────────────────────────┐
│ Claude Desktop / Claude Code / MCP Inspector / any MCP client     │
└─────────────────────────────┬─────────────────────────────────────┘
                              │ JSON-RPC over stdin/stdout
                              ▼
                  ┌───────────────────────┐
                  │  octa --mcp           │
                  │  (rmcp server)        │
                  └───────────┬───────────┘
                              │
                              ▼
                  ┌───────────────────────┐
                  │  FormatRegistry       │
                  │  • Parquet, CSV, JSON │
                  │  • SQLite, DuckDB     │
                  │  • Excel, SAS, …      │
                  └───────────────────────┘
```

The MCP server is a thin layer over Octa's existing format readers
and SQL engine. Adding a new file format to Octa automatically makes
it available to MCP, since the same `FormatRegistry` powers the GUI,
the CLI, and MCP.

## See also

- **[Setup](setup.md)** wires Octa into Claude Desktop, Claude Code,
  or MCP Inspector.
- **[Tools reference](tools/index.md)** covers input schemas,
  response formats, and examples for each tool.
- **[Limits & truncation](limits-and-truncation.md)** explains what
  happens when responses get big.
- **[Examples](examples.md)** shows worked prompts and how Claude
  tends to use the tools.
- **[Troubleshooting](troubleshooting.md)** covers what to do when
  things don't work.
