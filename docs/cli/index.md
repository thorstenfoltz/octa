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
octa --completions zsh          # shell completion script on stdout
```

The action flags are **mutually exclusive**, so pick one per
invocation. Trailing file arguments are ignored (with a warning)
when an action flag is set.

## Cloud objects as input

Anywhere a `FILE` argument is accepted, you can pass a cloud object URL
instead. No extra flag is involved: the path is recognised and read.

```bash
octa --schema s3://bucket/data.parquet
octa --head 20 gs://bucket/events.csv
```

The object is downloaded to a temporary file and read as usual.
Credentials come from a saved connection covering the URL, otherwise from
the ambient chain (`AWS_*` variables, a cached SSO session, `az login`,
Google application default credentials).

Cloud objects are **read-only** here: output still goes to a local path.
See [Cloud storage](cloud.md).

The same flags work identically across every distribution channel:
a plain binary off the releases page, an `install.sh` install, the
AUR package, or an AppImage. The AppImage is just the binary in a
self-contained bundle; invoke it directly:

```bash
./Octa-x86_64.AppImage --schema myfile.parquet
./Octa-x86_64.AppImage --mcp
```

A freshly downloaded AppImage has no permission to run yet, so give it
one first with `chmod 750 Octa-*-x86_64.AppImage`. See
[Installation](../getting-started/installation.md#appimage).

## Available actions

| Flag                                                 | Description                                   | Reference                                               |
|------------------------------------------------------|-----------------------------------------------|---------------------------------------------------------|
| `--schema FILE`                                      | Print column name + type as a table           | [→ `--schema`](schema.md)                               |
| `--head FILE [-n N]`                                 | Print the first N rows (default 20)           | [→ `--head`](head.md)                                   |
| `--tail FILE [-n N]`                                 | Print the last N rows (default 20)            | [→ `--tail`](tail.md)                                   |
| `--sample FILE [-n N] [--seed S]`                    | Print a reproducible random N-row sample      | [→ `--sample`](sample.md)                               |
| `--convert IN OUT`                                   | Convert between formats                       | [→ `--convert`](convert.md)                             |
| `--sql FILE -q '<query>'`                            | Run a SQL query against a file                | [→ `--sql`](sql.md)                                     |
| `--export-schema FILE [-t T]`                        | Render the schema as DDL / model / struct     | [→ `--export-schema`](export-schema.md)                 |
| `--compare-schemas A B`                              | Diff the schemas of two files                 | [→ `--compare-schemas`](compare-schemas.md)             |
| `--compare-distributions FILE --dist-column COL`     | Do two columns look like one population?      | [→ `--compare-distributions`](compare-distributions.md) |
| `--check-references PARENT --parent-column COL`      | Orphan child rows (exit 1 = orphans found)    | [→ `--check-references`](check-references.md)           |
| `--diff A B`                                         | Row-level diff: rows unique to each file      | [→ `--diff`](diff.md)                                   |
| `--describe FILE`                                    | One-shot snapshot: format + schema + sample   | [→ `--describe`](describe.md)                           |
| `--validate-schema FILE --expect-schema SCHEMA`      | Validate against JSON Schema (exit 1 = drift) | [→ `--validate-schema`](validate-schema.md)             |
| `--unique-columns FILE`                              | Find PK candidates (singles + combos)         | [→ `--unique-columns`](unique-columns.md)               |
| `--anonymize SPEC FILE`                              | Mask / scramble columns per a JSON spec       | [→ `--anonymize`](anonymize.md)                         |
| `--dedupe FILE`                                      | Remove duplicate rows                         | [→ `--dedupe`](dedupe.md)                               |
| `--impute COL=STRATEGY FILE`                         | Fill missing cells in a column                | [→ `--impute`](impute.md)                               |
| `--outliers FILE`                                    | Flag numeric outlier cells                    | [→ `--outliers`](outliers.md)                           |
| `--detect-pii FILE`                                  | Find likely personal-data columns             | [→ `--detect-pii`](pii.md)                              |
| `--union FILE --union-file FILE`                     | Stack files into one table                    | [→ `--union`](union.md)                                 |
| `--join FILE --join-file FILE --join-on COLS`        | Join files on key columns                     | [→ `--join`](join.md)                                   |
| `--partition-by COL --out-dir DIR FILE`              | One file per distinct column value            | [→ `--partition-by`](partition.md)                      |
| `--batch-convert --to EXT --out-dir DIR FILE...`     | Convert many files into one format            | [→ `--batch-convert`](batch-convert.md)                 |
| `--resample COL --value-cols COLS FILE`              | Group rows into time buckets and aggregate    | [→ `--resample`](timeseries.md)                         |
| `--rolling COL --order-by COL --window N FILE`       | Rolling aggregate over the previous N rows    | [→ `--rolling`](timeseries.md)                          |
| `--schema-drift DIR`                                 | Which files in a folder disagree on columns   | [→ `--schema-drift`](schema-drift.md)                   |
| `--drift-report A B`                                 | How two versions of one dataset differ        | [→ `--drift-report`](drift-report.md)                   |
| `--check FILE --rules RULES.toml`                    | Check values against a rules file             | [→ `--check`](check.md)                                 |
| `--relationships DIR`                                | Rank how the tables in a folder connect       | [→ `--relationships`](relationships.md)                 |
| `--harmonise-schema DIR --out-dir DIR`               | Rewrite a folder to one common schema         | [→ guide](../usage/harmonise-schemas.md)                |
| `--report OUT.html FILE`                             | Write a self-contained profiling report       | [→ `--report`](report.md)                               |
| `--fuzzy-join FILE --fuzzy-join-file FILE`           | Join on similarity rather than equality       | [→ `--fuzzy-join`](fuzzy-join.md)                       |
| `--to-workbook OUT.xlsx FILE...`                     | Write several files as one workbook           | [→ guide](../usage/saving.md)                           |
| `--sync-sql FILE --sync-table T --sync-on COLS`      | Generate the SQL that would sync a table      | [→ guide](../usage/database-connections.md)             |
| `--mcp`                                              | Start the MCP server                          | [→ MCP guide](../mcp/index.md)                          |
| `--cloud-ls URL`                                     | List a bucket or prefix                       | [→ cloud storage](cloud.md)                             |
| `--cloud-get URL --out PATH`                         | Download one cloud object                     | [→ cloud storage](cloud.md)                             |
| `--cloud-put PATH --to URL`                          | Upload a local file                           | [→ cloud storage](cloud.md)                             |
| `--cloud-copy URL --to URL`                          | Copy an object or prefix, across clouds too   | [→ cloud storage](cloud.md)                             |
| `--cloud-move URL --to URL`                          | Copy then delete the source                   | [→ cloud storage](cloud.md)                             |
| `--cloud-delete URL`                                 | Delete an object or prefix                    | [→ cloud storage](cloud.md)                             |
| `--db-tables --db CONN`                              | List a live connection's schemas and tables   | [→ guide](../usage/database-connections.md)             |
| `--db-query SQL --db CONN`                           | Run SQL on a live database server             | [→ guide](../usage/database-connections.md)             |
| `--db-write-table SCHEMA.TABLE --db CONN FILE`       | Write a file into a live database table       | [→ guide](../usage/database-connections.md)             |
| `--db-copy SCHEMA.TABLE --db CONN --db-copy-to CONN` | Copy a table server to server                 | [→ guide](../usage/database-connections.md)             |
| `--list-connections`                                 | List saved cloud and database connections     | [→ man page](man-page.md)                               |
| `--add-connection SPEC`                              | Add or replace a saved connection             | [→ man page](man-page.md)                               |
| `--remove-connection NAME`                           | Delete a saved connection and its secret      | [→ man page](man-page.md)                               |
| `--completions SHELL`                                | Print a shell completion script to stdout     | [→ completions](completions.md)                         |

`--export-schema` also has the short alias `-e`.

## Global options

These apply across actions (where they make sense):

| Flag                      | Applies to                                                                                                                                | Default     | Meaning                                                                                                                                                                 |
|---------------------------|-------------------------------------------------------------------------------------------------------------------------------------------|-------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `-f`, `--format` _FORMAT_ | `--schema`, `--head`, `--tail`, `--sample`, `--sql`, `--compare-schemas`, `--diff`, `--describe`, `--validate-schema`, `--unique-columns` | `tsv`       | Output format: `tsv`, `json`, or `csv`. Ignored by `--convert`, `--export-schema`, and `--mcp`.                                                                         |
| `-n`, `--lines` _N_       | `--head`, `--tail`, `--sample`                                                                                                            | `20`        | Number of rows to print / sample.                                                                                                                                       |
| `--seed` _N_              | `--sample`                                                                                                                                | `0`         | RNG seed for `--sample`; same seed + file yields the same sample.                                                                                                       |
| `-q`, `--query` _QUERY_   | `--sql`                                                                                                                                   | (required)  | Required for `--sql`. The query string; reference the file as `data`.                                                                                                   |
| `--sql-table NAME=PATH`   | `--sql`                                                                                                                                   | (none)      | Register an extra file as a workspace table named `NAME`. Repeatable. Any supported format.                                                                             |
| `--sql-attach ALIAS=PATH` | `--sql`                                                                                                                                   | (none)      | `ATTACH` a DuckDB or SQLite database under `ALIAS`. Repeatable.                                                                                                         |
| `--sql-write-to PATH`     | `--sql`                                                                                                                                   | (none)      | Persist the SELECT result to a DuckDB / SQLite file instead of printing it. Requires `--sql-write-table`.                                                               |
| `--sql-write-table TABLE` | `--sql-write-to`                                                                                                                          | (required)  | Target table name for `--sql-write-to`.                                                                                                                                 |
| `--sql-write-schema NAME` | `--sql-write-to`                                                                                                                          | `main`      | Target schema (DuckDB only). Leave unset or `main` for SQLite.                                                                                                          |
| `--sql-write-mode MODE`   | `--sql-write-to`                                                                                                                          | `create`    | `create`, `replace`, or `append`.                                                                                                                                       |
| `-t`, `--target` _TARGET_ | `--export-schema`                                                                                                                         | `postgres`  | Schema-export target: `postgres`, `mysql`, `sqlite`, `databricks`, `snowflake`, `pydantic`, `typescript`, `json-schema`, `rust`.                                        |
| `--table-a NAME`          | `--compare-schemas`                                                                                                                       | (no value)  | Specific table on FILE_A (multi-table sources only).                                                                                                                    |
| `--table-b NAME`          | `--compare-schemas`                                                                                                                       | (no value)  | Specific table on FILE_B (multi-table sources only).                                                                                                                    |
| `--table NAME`            | `--validate-schema`, `--describe`, `--unique-columns`                                                                                     | (no value)  | Specific table on FILE (multi-table sources).                                                                                                                           |
| `--expect-schema FILE`    | `--validate-schema`                                                                                                                       | (required)  | Path to the expected JSON Schema. Required by `--validate-schema`.                                                                                                      |
| `--sample-rows N`         | `--describe`                                                                                                                              | `5`         | Sample-row count for the preview. Clamped to `[0, 100]`.                                                                                                                |
| `--diff-db CONN`          | `--diff`                                                                                                                                  | (no value)  | Compare against a live database table instead of a second file. Names a saved connection.                                                                               |
| `--diff-db-table TABLE`   | `--diff-db`                                                                                                                               | (required)  | Table on that connection, as `SCHEMA.TABLE` or `CATALOG.SCHEMA.TABLE`.                                                                                                  |
| `--compression CODEC`     | `--convert`, `--batch-convert`                                                                                                            | (settings)  | Parquet codec: `uncompressed`, `snappy`, `zstd`, `gzip`, `lz4`. Ignored by other targets.                                                                               |
| `--row-group-size N`      | `--convert`, `--batch-convert`                                                                                                            | (settings)  | Rows per Parquet row group.                                                                                                                                             |
| `--deep`                  | `--describe`                                                                                                                              | off         | Also report the file's physical layout: row groups, compression, encodings, column statistics. Parquet only.                                                            |
| `--max-combo N`           | `--unique-columns`                                                                                                                        | `1`         | Max combo size to test (clamped to `[1, 3]`).                                                                                                                           |
| `--rows` _N_\|`all`       | `--schema`, `--head`, `--convert`, `--sql`                                                                                                | `5,000,000` | Override the streaming initial-load row cap for this invocation. Pass a number (commas / underscores OK) or `all` to load every row.                                    |
| `-h`, `--help`            | always                                                                                                                                    | (no value)  | Print the full help text (with worked examples) and exit. `-h` and `--help` produce the **same long-form output**.                                                      |
| `--version`               | always                                                                                                                                    | (no value)  | Print the Octa version and exit.                                                                                                                                        |
| `--stream`                | `--sql`                                                                                                                                   | off         | Let DuckDB scan the file where it lies instead of loading its rows, so an aggregate covers every row of a file larger than memory. Other actions say it does not apply. |

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
- `1` also as a **deliberate signal**, for the actions built to gate a
  pipeline: `--validate-schema` when the schema drifted,
  `--schema-drift` when the files disagree, `--harmonise-schema` when a
  file was refused, `--batch-convert` when an input failed,
  `--drift-report` when a `--fail-on` gate was breached, and `--check`
  on any failing or unrunnable rule.

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

## Progress on long runs

The actions that work through many items report progress on **stderr**, on one
line that rewrites itself:

```text
[137/500] products-2024-05.csv  ETA 1:12
```

`--batch-convert`, `--harmonise-schema` and `--partition-by` count items and
show an ETA once the first one is done. `--db-copy` has no total to count
against (knowing it would mean a `COUNT(*)` scan before the copy starts), so it
reports the running row count instead, and only on the universal lane: a
Postgres-to-Postgres copy runs as a single statement inside DuckDB, which
either finishes or does not.

The line is **written only when stderr is a terminal**. Redirect stderr, or run
in CI, and you get exactly the summary lines you always got, with no carriage
returns in the log. Nothing about stdout changes either way, so a pipeline
still receives clean data.

## Using octa in a pipeline

`-` means standard input where a file is expected, and standard output as the
`--convert` target:

```bash
# Read from a pipe; the format is worked out from the bytes, not a file name
curl -s https://example.org/sales.csv | octa --schema -

# Query without writing a file at all
cat sales.csv | octa --sql - -q "SELECT city, SUM(amount) FROM data GROUP BY city"

# Convert on the way through
cat sales.csv | octa --convert - - --to json | jq '.[0]'
```

`--to` is required when writing to `-`, because a pipe has no file name to
infer a format from. Counts and notes go to stderr, so the next command in the
pipeline receives exactly the data.

Piped input is buffered to a temporary file first. Every reader needs to seek
(a Parquet footer sits at the end of the file), so a pipe cannot be handed
straight to one. That is the honest cost of `-`: a pipe bigger than the
temporary filesystem will fail.
