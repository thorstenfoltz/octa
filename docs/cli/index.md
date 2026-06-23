# Command Line

Octa doubles as a small command-line tool. With no flags it launches
the GUI; with one of the **action flags** it performs that action
against a file and exits.

```bash
octa                            # launch GUI (empty window)
octa file1.csv file2.json       # launch GUI, open both files in tabs

octa --schema data.parquet      # action: print schema
octa --head data.csv -n 50      # action: first 50 rows
octa --tail data.csv -n 50      # action: last 50 rows
octa --sample data.csv -n 50 --seed 1   # reproducible random sample
octa --convert in.csv out.parquet
octa --sql data.parquet -q 'SELECT count(*) FROM data'
octa --sql sales.parquet --sql-table customers=customers.csv \
     -q 'SELECT c.name, SUM(s.amount) FROM data s JOIN customers c ON s.cid=c.cid GROUP BY c.name'
octa --export-schema data.parquet -t snowflake
octa --compare-schemas v1.parquet v2.parquet
octa --diff v1.parquet v2.parquet
octa --describe data.parquet
octa --validate-schema data.parquet --expect-schema expected.json
octa --unique-columns users.csv --max-combo 2
octa --mcp                      # MCP server on stdio
```

The action flags are **mutually exclusive**, so pick one per
invocation. Trailing file arguments are ignored (with a warning)
when an action flag is set.

The same flags work identically across every distribution channel:
a plain binary off the releases page, an `install.sh` install, the
AUR package, or an AppImage. The AppImage is just the binary in a
self-contained bundle; invoke it directly:

```bash
./Octa-x86_64.AppImage --schema myfile.parquet
./Octa-x86_64.AppImage --mcp
```

## Available actions

| Flag                                            | Description                                   | Reference                                   |
|-------------------------------------------------|-----------------------------------------------|---------------------------------------------|
| `--schema FILE`                                 | Print column name + type as a table           | [→ `--schema`](schema.md)                   |
| `--head FILE [-n N]`                            | Print the first N rows (default 20)           | [→ `--head`](head.md)                       |
| `--tail FILE [-n N]`                            | Print the last N rows (default 20)            | [→ `--tail`](tail.md)                       |
| `--sample FILE [-n N] [--seed S]`               | Print a reproducible random N-row sample      | [→ `--sample`](sample.md)                   |
| `--convert IN OUT`                              | Convert between formats                       | [→ `--convert`](convert.md)                 |
| `--sql FILE -q '<query>'`                       | Run a SQL query against a file                | [→ `--sql`](sql.md)                         |
| `--export-schema FILE [-t T]`                   | Render the schema as DDL / model / struct     | [→ `--export-schema`](export-schema.md)     |
| `--compare-schemas A B`                         | Diff the schemas of two files                 | [→ `--compare-schemas`](compare-schemas.md) |
| `--diff A B`                                    | Row-level diff: rows unique to each file      | [→ `--diff`](diff.md)                       |
| `--describe FILE`                               | One-shot snapshot: format + schema + sample   | [→ `--describe`](describe.md)               |
| `--validate-schema FILE --expect-schema SCHEMA` | Validate against JSON Schema (exit 1 = drift) | [→ `--validate-schema`](validate-schema.md) |
| `--unique-columns FILE`                         | Find PK candidates (singles + combos)         | [→ `--unique-columns`](unique-columns.md)   |
| `--anonymize SPEC FILE`                         | Mask / scramble columns per a JSON spec       | [→ `--anonymize`](anonymize.md)             |
| `--dedupe FILE`                                 | Remove duplicate rows                         | [→ `--dedupe`](dedupe.md)                   |
| `--impute COL=STRATEGY FILE`                    | Fill missing cells in a column                | [→ `--impute`](impute.md)                   |
| `--outliers FILE`                               | Flag numeric outlier cells                    | [→ `--outliers`](outliers.md)               |
| `--detect-pii FILE`                             | Find likely personal-data columns             | [→ `--detect-pii`](pii.md)                  |
| `--union FILE --union-file FILE`                | Stack files into one table                    | [→ `--union`](union.md)                     |
| `--join FILE --join-file FILE --join-on COLS`   | Join files on key columns                     | [→ `--join`](join.md)                       |
| `--partition-by COL --out-dir DIR FILE`         | One file per distinct column value            | [→ `--partition-by`](partition.md)          |
| `--mcp`                                         | Start the MCP server                          | [→ MCP guide](../mcp/index.md)              |

`--export-schema` also has the short alias `-e`.

## Global options

These apply across actions (where they make sense):

| Flag                      | Applies to                                                                                                                                | Default     | Meaning                                                                                                                              |
|---------------------------|-------------------------------------------------------------------------------------------------------------------------------------------|-------------|--------------------------------------------------------------------------------------------------------------------------------------|
| `-f`, `--format` _FORMAT_ | `--schema`, `--head`, `--tail`, `--sample`, `--sql`, `--compare-schemas`, `--diff`, `--describe`, `--validate-schema`, `--unique-columns` | `tsv`       | Output format: `tsv`, `json`, or `csv`. Ignored by `--convert`, `--export-schema`, and `--mcp`.                                      |
| `-n`, `--lines` _N_       | `--head`, `--tail`, `--sample`                                                                                                            | `20`        | Number of rows to print / sample.                                                                                                    |
| `--seed` _N_              | `--sample`                                                                                                                                | `0`         | RNG seed for `--sample`; same seed + file yields the same sample.                                                                    |
| `-q`, `--query` _QUERY_   | `--sql`                                                                                                                                   | (required)  | Required for `--sql`. The query string; reference the file as `data`.                                                                |
| `--sql-table NAME=PATH`   | `--sql`                                                                                                                                   | (none)      | Register an extra file as a workspace table named `NAME`. Repeatable. Any supported format.                                          |
| `--sql-attach ALIAS=PATH` | `--sql`                                                                                                                                   | (none)      | `ATTACH` a DuckDB or SQLite database under `ALIAS`. Repeatable.                                                                      |
| `--sql-write-to PATH`     | `--sql`                                                                                                                                   | (none)      | Persist the SELECT result to a DuckDB / SQLite file instead of printing it. Requires `--sql-write-table`.                            |
| `--sql-write-table TABLE` | `--sql-write-to`                                                                                                                          | (required)  | Target table name for `--sql-write-to`.                                                                                              |
| `--sql-write-schema NAME` | `--sql-write-to`                                                                                                                          | `main`      | Target schema (DuckDB only). Leave unset or `main` for SQLite.                                                                       |
| `--sql-write-mode MODE`   | `--sql-write-to`                                                                                                                          | `create`    | `create`, `replace`, or `append`.                                                                                                    |
| `-t`, `--target` _TARGET_ | `--export-schema`                                                                                                                         | `postgres`  | Schema-export target: `postgres`, `mysql`, `sqlite`, `databricks`, `snowflake`, `pydantic`, `typescript`, `json-schema`, `rust`.     |
| `--table-a NAME`          | `--compare-schemas`                                                                                                                       | (no value)  | Specific table on FILE_A (multi-table sources only).                                                                                 |
| `--table-b NAME`          | `--compare-schemas`                                                                                                                       | (no value)  | Specific table on FILE_B (multi-table sources only).                                                                                 |
| `--table NAME`            | `--validate-schema`, `--describe`, `--unique-columns`                                                                                     | (no value)  | Specific table on FILE (multi-table sources).                                                                                        |
| `--expect-schema FILE`    | `--validate-schema`                                                                                                                       | (required)  | Path to the expected JSON Schema. Required by `--validate-schema`.                                                                   |
| `--sample-rows N`         | `--describe`                                                                                                                              | `5`         | Sample-row count for the preview. Clamped to `[0, 100]`.                                                                             |
| `--max-combo N`           | `--unique-columns`                                                                                                                        | `1`         | Max combo size to test (clamped to `[1, 3]`).                                                                                        |
| `--rows` _N_\|`all`       | `--schema`, `--head`, `--convert`, `--sql`                                                                                                | `5,000,000` | Override the streaming initial-load row cap for this invocation. Pass a number (commas / underscores OK) or `all` to load every row. |
| `-h`, `--help`            | always                                                                                                                                    | (no value)  | Print the full help text (with worked examples) and exit. `-h` and `--help` produce the **same long-form output**.                   |
| `--version`               | always                                                                                                                                    | (no value)  | Print the Octa version and exit.                                                                                                     |

## Output formatting

The `-f / --format` flag controls the output format for every action
that prints a table:

| Value             | Format                                  | Notes                                                          |
|-------------------|-----------------------------------------|----------------------------------------------------------------|
| `tsv` _(default)_ | Tab-separated values                    | Most shell tools (`awk`, `column`, `sort`) parse TSV natively  |
| `json`            | JSON array of `{column: value}` objects | Pretty-printed; numeric / boolean cells keep their native type |
| `csv`             | RFC 4180 CSV                            | Fields with comma / quote / newline are properly quoted        |

```bash
octa --schema data.parquet              # TSV
octa --schema data.parquet -f json      # JSON
octa --schema data.parquet -f csv       # CSV
```

The format flag applies to `--schema`, `--head`, and `--sql`.
`--convert` chooses the output format from the **extension** of the
output path; `--export-schema` emits source code chosen by `-t`; `-f`
has no effect for either.

## Help output

```bash
octa --help       # full reference with worked examples
octa -h           # same: Octa wires both flags to the long-form output
```

The help text includes worked examples for every action, so
`octa --help` is a good first stop if you forget a flag.

## Exit codes

- `0` on success.
- `1` on any error: invalid arguments, file-not-found, read /
  parse failure, conversion target rejected, etc.

Errors are written to **stderr**; tabular output goes to **stdout**.
This means you can safely pipe Octa's output through `jq`, `awk`,
`xsv`, etc. without errors corrupting the data stream.

## Man page

Two consumption paths for the same content:

- **In a terminal**: `man octa` after installing Octa via
  `install.sh`, the AUR (`octa` / `octa-bin`), or the Linux release
  tarball. The release pipeline runs `asciidoctor` to render the
  page and `install.sh` drops it into
  `$PREFIX/share/man/man1/octa.1`. See
  [Installation](../getting-started/installation.md) for details.
- **On this site**: the [Man Page](man-page.md) page mirrors the
  same content as Markdown, with cross-links to the rest of the
  docs.

The canonical source is
[`docs/cli/octa.1.adoc`](https://github.com/thorstenfoltz/octa/blob/master/docs/cli/octa.1.adoc)
(AsciiDoc). To render it manually:

```bash
asciidoctor -b manpage docs/cli/octa.1.adoc -o octa.1
man ./octa.1                            # preview without installing
```

## See also

- The dedicated [`--schema`](schema.md), [`--head`](head.md),
  [`--convert`](convert.md), [`--sql`](sql.md),
  [`--export-schema`](export-schema.md),
  [`--compare-schemas`](compare-schemas.md),
  [`--describe`](describe.md),
  [`--validate-schema`](validate-schema.md), and
  [`--unique-columns`](unique-columns.md) pages cover each action in
  detail.
- [Man page reference](man-page.md) is a single-page, terminal-style
  reference matching `man octa`.
- [MCP server guide](../mcp/index.md) for `--mcp`.
- [Workflows & recipes](../tips/workflows.md) for chained-CLI
  examples (CSV → Parquet pipelines, JSON-line filtering, etc.).
