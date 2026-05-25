# Man Page Reference

The full man-page reference for `octa(1)`. The source of truth is
[`docs/cli/octa.1.adoc`](https://github.com/thorstenfoltz/octa/blob/master/docs/cli/octa.1.adoc)
(AsciiDoc); this page mirrors that content as Markdown for the
docs site.

On Linux (with the man page installed) `man octa` gives you the
same content at a terminal. See
[Installation](../getting-started/installation.md) for how to get
the man page on disk.

## Name

**octa**: multi-format tabular data viewer, editor, CLI tool, and
MCP server.

## Synopsis

```text
octa [FILE...]
octa --schema FILE [-f FORMAT] [--rows N|all]
octa --head FILE [-n N] [-f FORMAT] [--rows N|all]
octa --convert IN OUT [--rows N|all]
octa --sql FILE -q QUERY [-f FORMAT] [--rows N|all]
octa --export-schema FILE [-t TARGET]
octa --mcp
```

## Description

**octa** is a desktop application for viewing and editing tabular
data files. It opens Parquet, CSV, JSON, SQLite, DuckDB, Excel, and
roughly twenty more formats in a spreadsheet-like view with sorting,
filtering, full-text search, inline editing, SQL queries, and file
comparison.

When invoked with no flags, it launches the graphical interface,
optionally opening the supplied *FILE*(s) in tabs. When invoked
with one of the action flags (`--schema`, `--head`, `--convert`,
`--sql`, `--export-schema`, `--mcp`), it performs that action and
exits.

Action flags are **mutually exclusive**. Trailing *FILE* arguments
are ignored (with a warning) when an action flag is set.

## Action Flags

`--schema FILE`
:   Print the column schema of *FILE* as a two-column table
    (column name, data type). For streaming formats (Parquet, CSV,
    TSV) the reader loads the initial-row batch (**5,000,000 rows**
    by default) and projects the schema from that. See
    [`octa --schema`](schema.md) for the dedicated page.

`--head FILE`
:   Print the first *N* rows of *FILE* to standard output. *N*
    defaults to 20 and is set with `-n` / `--lines`. For streaming
    formats, the reader stops at the initial-load cap and *N* is a
    slice off that. See [`octa --head`](head.md).

`--convert IN OUT`
:   Convert *IN* to *OUT*. Both formats are inferred from each
    path's extension and routed through the shared format registry.
    Read-only output formats (SAS, R datasets, HDF5, NetCDF, EPUB,
    GeoJSON) are rejected with a clear error. Conversion is bounded
    by the initial-load cap (5 M rows by default); pass `--rows all`
    to convert the full file. See [`octa --convert`](convert.md).

`--sql FILE`
:   Run a SQL query against *FILE*. The query is supplied via
    `-q` / `--query`. *FILE* is exposed to DuckDB as a temporary
    table called **data**. Mutations (INSERT / UPDATE / DELETE) do
    **not** persist back to *FILE*, because the in-memory DuckDB
    connection is discarded at exit. See [`octa --sql`](sql.md).

`--export-schema FILE`
:   Render *FILE*'s column schema as SQL DDL, a Pydantic model, a
    TypeScript interface, a JSON Schema document, or a Rust struct,
    and print it to standard output. The target is chosen with `-t`
    / `--target` (default `postgres`); only the column list is read.
    See [`octa --export-schema`](export-schema.md).

`--mcp`
:   Start a Model Context Protocol (MCP) server on standard
    input / output. Eleven tools are exposed: `read_table`,
    `schema`, `list_tables`, `count_rows`, `run_sql`, `convert`,
    `export_schema`, `profile`, `find_duplicates`,
    `value_frequency`, `search`. Defaults for the row limit and
    per-cell byte cap come from the user's Octa settings
    ([Settings → MCP](../reference/settings.md#mcp)). See the
    [MCP server guide](../mcp/index.md) for setup.

## Options

`-n N`, `--lines N`
:   Row count for `--head`. Default **20**.

`-q QUERY`, `--query QUERY`
:   SQL query string for `--sql`. Always reference the file's data
    as the table **data**.

`-t TARGET`, `--target TARGET`
:   Output target for `--export-schema`. *TARGET* is one of
    `postgres` *(default)*, `mysql`, `sqlite`, `databricks`,
    `snowflake`, `pydantic`, `typescript`, `json-schema`, or `rust`.

`--rows N|all`
:   Override the initial-load row cap for this invocation. Streaming
    formats (Parquet, CSV, TSV) honour a process-wide cap (default
    5,000,000 rows); `--rows 10,000,000` raises it, `--rows all`
    disables it entirely. Applies to `--schema`, `--head`,
    `--convert`, and `--sql`. Commas / underscores in the number
    are allowed for readability.

`-f FORMAT`, `--format FORMAT`
:   Output format for actions that print a table. *FORMAT* is one
    of:

    - `tsv` *(default)*: tab-separated values, one row per line,
      header row first. TAB and newline characters in cells are
      replaced with spaces (TSV has no escape mechanism).
    - `json`: pretty-printed JSON array of `{column: value}`
      objects. Numeric and boolean cells keep their native JSON
      types; dates, blobs, and nested values become strings.
    - `csv`: RFC 4180 CSV. Fields with comma, quote, or newline
      are properly quoted; embedded quotes are doubled.

    `--format` has no effect for `--convert` (output format is taken
    from the output path's extension), `--export-schema` (which emits
    source code chosen by `-t`), or `--mcp`.

`-h`, `--help`
:   Print the full help text (worked examples for every action)
    and exit. `-h` and `--help` produce the **same long-form
    output**, because Octa intentionally wires both flags to the
    same help text rather than using clap's default short/long
    split.

`--version`
:   Print the Octa version and exit.

## Output Streams

Tabular data is written to **stdout**. Status messages, warnings,
and errors are written to **stderr**. This means
`octa --sql FILE -q QUERY -f json | jq ...` is safe even when an
error occurs, since the data stream stays clean.

Exit code is **0** on success and **1** on any error (invalid
arguments, file-not-found, parse failure, write rejection, etc.).

## Examples

Open multiple files in the GUI:

```bash
octa file1.csv file2.parquet file3.json
```

Print the schema of a Parquet file:

```bash
octa --schema sales.parquet
```

Print the first 5 rows of a CSV as JSON:

```bash
octa --head data.csv -n 5 -f json
```

Convert formats:

```bash
octa --convert in.csv out.parquet
octa --convert workbook.xlsx tidy.sqlite
```

Group-by aggregation:

```bash
octa --sql sales.parquet -q 'SELECT region, SUM(amount) FROM data GROUP BY region'
```

Read every row of a huge file:

```bash
octa --sql huge.parquet -q 'SELECT count(*) FROM data' --rows all
octa --head huge.parquet -n 100 --rows 10,000,000
```

Pipe a SQL result through `jq`:

```bash
octa --sql users.parquet -q 'SELECT email FROM data WHERE active' -f json \
  | jq -r '.[].email'
```

Export a schema as Snowflake DDL or a Pydantic model:

```bash
octa --export-schema sales.parquet -t snowflake
octa -e users.parquet -t pydantic > users_model.py
```

Start the MCP server:

```bash
octa --mcp
```

## Files

`$XDG_CONFIG_HOME/octa/settings.toml`
:   Linux. User settings. Created on first launch with defaults.
    See [Settings reference](../reference/settings.md) for every
    key.

`$HOME/Library/Application Support/Octa/settings.toml`
:   macOS. Same purpose.

`%APPDATA%\Octa\settings.toml`
:   Windows. Same purpose.

## MCP Server

When invoked with `--mcp`, Octa speaks the Model Context Protocol
over JSON-RPC on stdin/stdout. Eleven tools are exposed:

- [`read_table(path, limit?, unlimited?, table?)`](../mcp/tools/read_table.md)
  returns schema + rows JSON.
- [`schema(path, table?)`](../mcp/tools/schema.md) returns column
  schema only.
- [`list_tables(path)`](../mcp/tools/list_tables.md) lists tables
  for multi-table sources (SQLite / DuckDB / GeoPackage).
- [`count_rows(path, unlimited?, table?)`](../mcp/tools/count_rows.md)
  returns the row count for a tabular file.
- [`run_sql(path, query, limit?, unlimited?, table?)`](../mcp/tools/run_sql.md)
  runs DuckDB against the file as table `data`.
- [`convert(input, output, unlimited?, table?)`](../mcp/tools/convert.md)
  exposes the same surface as `--convert`.
- [`export_schema(path, target, table?)`](../mcp/tools/export_schema.md)
  renders the schema as DDL / a model / a struct.
- [`profile(path, unlimited?, table?)`](../mcp/tools/profile.md)
  returns per-column statistics via `SUMMARIZE`.
- [`find_duplicates(path, key_columns, …, unlimited?)`](../mcp/tools/find_duplicates.md)
  returns rows sharing key-column values.
- [`value_frequency(path, column, …, unlimited?)`](../mcp/tools/value_frequency.md)
  counts per-column values.
- [`search(path, query, mode?, …, unlimited?)`](../mcp/tools/search.md)
  matches cells across every column.

Defaults (the response row cap of 1000 rows, per-cell byte cap of
64 KiB, and file-loader cap of 5,000,000 rows) are configurable
under [Settings → MCP](../reference/settings.md#mcp) and
Settings → Performance. They are read once at server startup;
changes require a restart. Per-call, pass `limit: 0` to lift the
response cap and `unlimited: true` to lift the file-loader cap so
the tool sees every row on disk. Parquet files with very many row
groups fall back to a DuckDB-backed reader automatically. See
[Limits & truncation](../mcp/limits-and-truncation.md) for the full
mechanics.

## See Also

`man(1)`, `jq(1)`, `duckdb(1)`, `parquet-tools(1)`

- Project homepage: <https://github.com/thorstenfoltz/octa>
- Online documentation: <https://thorstenfoltz.github.io/octa/>
- [Tips & recipes](../tips/workflows.md) covers worked CLI
  workflows (CSV → Parquet pipelines, JSON-line filtering, etc.).

## Bugs / Feedback

Report bugs at <https://github.com/thorstenfoltz/octa/issues>.

## Author

Thorsten Foltz

## Copyright

Copyright © 2026 Thorsten Foltz. Licensed under the MIT
license.
