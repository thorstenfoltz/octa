# `octa --schema-drift`

Scan a folder and report which files disagree about their columns. Exit
code is `0` when every file agrees and `1` when they do not, so it slots
straight into a CI pipeline the same way
[`--validate-schema`](validate-schema.md) does.

## Synopsis

```bash
octa --schema-drift DIR [--recursive] [--ignore-case] [-f FORMAT]
```

| Flag                    | Required | Meaning                                           |
|-------------------------|----------|---------------------------------------------------|
| `--schema-drift DIR`    | yes      | The folder to scan.                               |
| `--recursive`           | no       | Walk subdirectories, to a depth of 8.             |
| `--ignore-case`         | no       | Treat names differing only in case as one column. |
| `-f`, `--format FORMAT` | no       | Output format: `tsv` (default), `json`, or `csv`. |

## What it does

Files are **grouped by schema**, not compared pairwise. For a folder of
500 Parquet parts the useful answer is "497 look like this, 3 look like
that", not a 500-column matrix, so identical schemas collapse into one
variant and the variants are listed largest first. The odd file out is
then visibly the minority.

Only the schema is read, never the rows. Parquet reads its footer and
Arrow IPC its header, so scanning a large folder costs little; other
formats fall back to a full read to get their columns.

Column names are compared **case-sensitively by default**, because
differing case is a real difference that some downstream tools care
about. `--ignore-case` folds it away.

## Output

The report table goes to **stdout**: a `status` column, a `column`
column, then one column per variant holding that variant's type for the
column, or empty where the variant lacks it. Rows that need attention
sort above the consistent ones, since a scan is read top down.

| `status`       | Meaning                                             |
|----------------|-----------------------------------------------------|
| `type varies`  | Every variant has the column, with differing types. |
| `missing in N` | N variants do not have the column at all.           |
| `consistent`   | Same type everywhere.                               |

The per-variant **file lists**, any files that could not be read, and
the names of the drifting columns go to **stderr**, so a pipe stays
parseable. One unreadable file does not abort the scan: it is reported
as skipped and the other files are still compared.

## Examples

### A folder that agrees

```bash
$ octa --schema-drift data/parts/
status      column  variant_1 (12 files)
consistent  id      Int64
consistent  amount  Float64
$ echo $?
0
```

### One part drifted

```bash
$ octa --schema-drift data/parts/
status        column  variant_1 (11 files)  variant_2 (1 file)
type varies   amount  Float64               Utf8
consistent    id      Int64                 Int64
$ echo $?
1
```

Stderr names the file:

```
variant 1: 11 file(s)
  part-00000.parquet
  ...
variant 2: 1 file(s)
  part-00011.parquet
drifting columns: amount
```

### Plumbed into a CI step

```yaml
- name: Check the landing zone for schema drift
  run: octa --schema-drift data/landing/ --recursive
```

## Ceilings

- Local paths only. Cloud prefixes are not scanned.
- Multi-table sources (a workbook, a database file) report their first
  table, matching how the other folder-wide actions treat them.
- `--recursive` stops at depth 8, so a mistyped path cannot walk a whole
  home directory.

## See also

- [`octa --validate-schema`](validate-schema.md): one file against an
  expected JSON Schema, same exit-code contract.
- [`octa --compare-schemas`](compare-schemas.md): symmetric two-file
  diff.
- [`octa --union`](union.md): reconcile differing schemas into one
  table instead of only reporting them.
