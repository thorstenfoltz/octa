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
octa --tail FILE [-n N] [-f FORMAT] [--rows N|all]
octa --sample FILE [-n N] [--seed N] [-f FORMAT] [--rows N|all]
octa --convert IN OUT [--rows N|all]
octa --sql FILE -q QUERY [-f FORMAT] [--rows N|all]
     [--sql-table NAME=PATH ...] [--sql-attach ALIAS=PATH ...]
     [--sql-write-to PATH --sql-write-table TABLE
       [--sql-write-schema SCHEMA] [--sql-write-mode create|append|replace]]
octa --export-schema FILE [-t TARGET]
octa --compare-schemas FILE_A FILE_B [--table-a NAME] [--table-b NAME] [-f FORMAT]
octa --diff FILE_A FILE_B [--diff-mode MODE] [--diff-on COLS] [-f FORMAT]
octa --describe FILE [--table NAME] [--sample-rows N] [-f FORMAT]
octa --validate-schema FILE --expect-schema SCHEMA_FILE [--table NAME] [-f FORMAT]
octa --unique-columns FILE [--table NAME] [--max-combo N] [-f FORMAT]
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
with one of the action flags (`--schema`, `--head`, `--tail`,
`--sample`, `--convert`, `--sql`, `--export-schema`,
`--compare-schemas`, `--diff`, `--describe`, `--validate-schema`,
`--unique-columns`, `--mcp`), it performs that action and exits.

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

`--tail FILE`
:   Print the last *N* rows of *FILE*. *N* defaults to 20 (`-n` /
    `--lines`). Streaming formats load with the initial-row cap, so
    the tail reflects the end of the loaded window; raise `--rows` to
    tail the true end of a very large file.

`--sample FILE`
:   Print a random *N*-row sample of *FILE* (without replacement,
    original row order preserved). *N* defaults to 20 (`-n` /
    `--lines`); the sample is reproducible for a given `--seed`
    (default 0).

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
    table called **data**. Extra tables can be loaded with
    `--sql-table NAME=PATH` and whole DuckDB / SQLite databases can
    be attached with `--sql-attach ALIAS=PATH`, so a single
    invocation can JOIN across formats. The SELECT result can be
    persisted to a DuckDB or SQLite file via `--sql-write-to`.
    Mutations (INSERT / UPDATE / DELETE) on `data` itself do
    **not** persist back to *FILE*, because the in-memory DuckDB
    connection is discarded at exit. See [`octa --sql`](sql.md).

`--export-schema FILE`
:   Render *FILE*'s column schema as SQL DDL, a Pydantic model, a
    TypeScript interface, a JSON Schema document, or a Rust struct,
    and print it to standard output. The target is chosen with `-t`
    / `--target` (default `postgres`); only the column list is read.
    See [`octa --export-schema`](export-schema.md).

`--compare-schemas FILE_A FILE_B`
:   Diff the column schemas of two files. Prints a four-column
    table (`status` / `column` / `type_a` / `type_b`). Matching is
    by exact, case-sensitive column name. Use `--table-a` /
    `--table-b` to pick a specific table on multi-table sources.
    See [`octa --compare-schemas`](compare-schemas.md).

`--diff FILE_A FILE_B`
:   Compare two files. `--diff-mode` selects the strategy; the output
    is a table tagged by a leading `status` column, and a summary line
    (per-mode counts) goes to standard error. Complements
    `--compare-schemas`, which diffs only the column metadata.

`--diff-mode MODE`
:   Comparison strategy for `--diff` (default `set`): `set` compares
    rows by whole-row content (every column, positionally) and prints
    rows unique to each side (`only_in_a` / `only_in_b`). `ordered`
    lines up row *i* of A with row *i* of B and prints matched rows
    that differ as `changed` (with a `changed_columns` column) plus
    any trailing `only_in_a` / `only_in_b` rows. `join` matches rows on
    the `--diff-on` key column(s) and prints added / removed / changed
    rows.

`--diff-on COLS`
:   Key column(s) for `--diff-mode join`, comma-separated (matched by
    name). Required when the mode is `join`; ignored otherwise.

`--describe FILE`
:   Print a one-shot orientation snapshot of *FILE*: format, file
    size, row count, column schema, and a small sample of rows.
    Use `--sample-rows N` to change the preview size (default 5,
    max 100). See [`octa --describe`](describe.md).

`--validate-schema FILE`
:   Check *FILE*'s column schema against the JSON Schema given by
    `--expect-schema SCHEMA_FILE`. Exit code is `0` on a clean
    match and `1` otherwise, which is CI-pipeable. Schemas produced by
    `--export-schema -t json-schema` round-trip cleanly. See
    [`octa --validate-schema`](validate-schema.md).

`--unique-columns FILE`
:   Find columns (and optional small combinations) whose values are
    unique across *FILE*. Useful for primary-key reconnaissance.
    Use `--max-combo N` (clamped to `[1, 3]`) to test pairs /
    triples. See [`octa --unique-columns`](unique-columns.md).

`--mcp`
:   Start a Model Context Protocol (MCP) server on standard
    input / output. Twenty tools are exposed: `read_table`,
    `tail`, `sample`, `schema`, `list_tables`, `count_rows`,
    `run_sql`, `convert`, `export_schema`, `profile`,
    `find_duplicates`, `value_frequency`, `search`,
    `compare_schemas`, `diff_tables`, `describe_file`,
    `validate_against_schema`, `unique_columns`, `write_table`,
    `edit_table`. Defaults for the
    row limit and per-cell byte cap come from the user's Octa
    settings ([Settings → MCP](../reference/settings.md#mcp)). See
    the [MCP server guide](../mcp/index.md) for setup.

`--mcp-read-only`
:   Only valid with `--mcp`. Omits the file-writing tools
    (`write_table`, `edit_table`, `convert`) so the server exposes a
    read-only surface.

## Options

`-n N`, `--lines N`
:   Row count for `--head`, `--tail`, and `--sample`. Default **20**.

`--seed N`
:   Seed for `--sample`, for reproducible output. Default **0**.

`-q QUERY`, `--query QUERY`
:   SQL query string for `--sql`. Always reference the file's data
    as the table **data**.

`--sql-table NAME=PATH`
:   For `--sql` only. Register an extra file as a workspace table
    named *NAME*. Any supported format. Repeatable.

`--sql-attach ALIAS=PATH`
:   For `--sql` only. `ATTACH` a DuckDB or SQLite database under
    *ALIAS*. Repeatable. After attachment every inner table is
    queryable as `alias.schema.tbl` (DuckDB) or `alias.tbl`
    (SQLite when the DuckDB sqlite extension is bundled, otherwise
    fallback registration under `alias__table`).

`--sql-write-to PATH`
:   For `--sql` only. Persist the SELECT result to *PATH* instead
    of printing it. *PATH*'s extension picks DuckDB (`.duckdb`,
    `.ddb`) or SQLite (everything else). The file is created if
    missing. Requires `--sql-write-table`; `--sql-write-schema` and
    `--sql-write-mode` are optional.

`--sql-write-table TABLE`
:   Target table name for `--sql-write-to`.

`--sql-write-schema SCHEMA`
:   Target schema for `--sql-write-to`. DuckDB only; defaults to
    `main`. SQLite has no schemas; pass `main` or leave unset.

`--sql-write-mode MODE`
:   `create` (default; errors if the table already exists),
    `replace` (drop + recreate), or `append` (`INSERT` into the
    existing table). Column count and order must match in append
    mode.

`-t TARGET`, `--target TARGET`
:   Output target for `--export-schema`. *TARGET* is one of
    `postgres` *(default)*, `mysql`, `sqlite`, `databricks`,
    `snowflake`, `pydantic`, `typescript`, `json-schema`, or `rust`.

`--table-a NAME`, `--table-b NAME`
:   For `--compare-schemas` only: pick a specific table on each
    side when the source is multi-table (SQLite, DuckDB,
    GeoPackage).

`--table NAME`
:   For `--describe`, `--validate-schema`, and `--unique-columns`:
    pick a specific table on the file when the source is
    multi-table.

`--expect-schema SCHEMA_FILE`
:   Path to the expected JSON Schema for `--validate-schema`.
    Required by that action.

`--sample-rows N`
:   Number of preview rows for `--describe` (default 5, clamped to
    100).

`--max-combo N`
:   Max combo size for `--unique-columns` (default 1, clamped to
    `[1, 3]`).

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
`--validate-schema` also exits **1** on a successful read where
the schemas differ, so CI pipelines can gate on the schema directly.

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

Print the last rows / a reproducible random sample:

```bash
octa --tail data.csv -n 5
octa --sample data.parquet -n 20 --seed 1
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

JOIN across formats:

```bash
octa --sql sales.parquet \
     --sql-table customers=customers.csv \
     -q 'SELECT c.name, SUM(d.amount) FROM data d
         JOIN customers c ON d.cid = c.cid GROUP BY c.name'
```

ATTACH a DuckDB warehouse and JOIN against it:

```bash
octa --sql sales.parquet \
     --sql-attach wh=warehouse.duckdb \
     -q 'SELECT count(*) FROM data d
         JOIN wh.main.products p ON d.cid = p.cid'
```

Write a SQL result back to a DuckDB schema:

```bash
octa --sql sales.parquet -q '
  SELECT region, SUM(amount) AS total FROM data GROUP BY region
' --sql-write-to analytics.duckdb \
  --sql-write-schema reports \
  --sql-write-table q4_summary
```

Export a schema as Snowflake DDL or a Pydantic model:

```bash
octa --export-schema sales.parquet -t snowflake
octa -e users.parquet -t pydantic > users_model.py
```

Diff the schemas of two files:

```bash
octa --compare-schemas v1.parquet v2.parquet
octa --compare-schemas a.sqlite b.sqlite --table-a users --table-b users -f json
```

Diff two files' rows:

```bash
octa --diff v1.csv v2.csv
octa --diff v1.csv v2.csv --diff-mode ordered
octa --diff v1.csv v2.csv --diff-mode join --diff-on id
octa --diff a.parquet b.parquet -f json
```

One-shot file snapshot:

```bash
octa --describe data.parquet --sample-rows 3
octa --describe users.sqlite --table customers -f json
```

Validate a file against a JSON Schema in CI:

```bash
octa --export-schema sales.parquet -t json-schema > sales.schema.json
octa --validate-schema sales.parquet --expect-schema sales.schema.json
```

Find primary-key candidates:

```bash
octa --unique-columns users.csv
octa --unique-columns orders.parquet --max-combo 2 -f json
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
over JSON-RPC on stdin/stdout. Fifteen tools are exposed:

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
- [`compare_schemas(path_a, path_b, table_a?, table_b?)`](../mcp/tools/compare_schemas.md)
  diffs the column schemas of two files.
- [`describe_file(path, table?, sample_rows?, unlimited?)`](../mcp/tools/describe_file.md)
  returns a one-shot orientation snapshot.
- [`validate_against_schema(path, table?, schema_path?, schema_inline?)`](../mcp/tools/validate_against_schema.md)
  checks a file against a JSON Schema.
- [`unique_columns(path, table?, max_combo_size?, unlimited?)`](../mcp/tools/unique_columns.md)
  finds primary-key candidates.
- [`write_table(path, columns, rows?, mode?, unlimited?)`](../mcp/tools/write_table.md)
  writes inline rows to a new file (create / overwrite / append).
- [`edit_table(path, table?, set?, insert_rows?, delete_rows?, unlimited?)`](../mcp/tools/edit_table.md)
  edits an existing file in place (DB sources diff-saved).

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
