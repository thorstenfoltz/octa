# `octa --diff`

Compare two files and print what differs. Where
[`--compare-schemas`](compare-schemas.md) diffs the column *metadata*,
`--diff` diffs the actual *rows*. Three modes (`--diff-mode`) trade off
how rows are matched, from coarse whole-row membership to precise
cell-level change detection.

## Synopsis

```bash
octa --diff FILE_A FILE_B [--diff-mode MODE] [--diff-on COLS] [-f FORMAT]
```

| Flag                    | Required        | Meaning                                                              |
|-------------------------|-----------------|----------------------------------------------------------------------|
| `--diff A B`            | yes             | The two files to compare. Exactly two paths.                         |
| `--diff-mode MODE`      | no              | `set` (default), `ordered`, or `join`. See below.                    |
| `--diff-on COLS`        | for `join` only | Key column(s), comma-separated (e.g. `--diff-on id` or `id,region`). |
| `-f`, `--format FORMAT` | no              | Output format: `tsv` (default), `json`, or `csv`.                    |

## Modes

### `set` (default) - whole-row membership

Each row is keyed by its **whole-row content**: every column, in order,
rendered to text and joined. A row in A matches a row in B when their
keys are equal. Two consequences follow:

- **Columns are positional.** The two files should share the same
  column order (and, ideally, names) for the diff to be meaningful.
  A reordered column set will report everything as changed.
- **Cross-format works.** Because matching is on rendered values, a CSV
  row and a Parquet row with the same logical content match. You can
  diff `before.csv` against `after.parquet`.

You learn *which whole rows* are unique to each side, but not which
cells changed within a row.

### `ordered` - positional, cell-level

Row `i` of A is lined up with row `i` of B and compared cell by cell
over the shared columns. Matched rows that differ are reported with a
`changed_columns` field naming the differing fields; trailing rows on
the longer side are tagged `only_in_a` / `only_in_b`. Use this when the
two files are in the **same order** and you want to see exactly which
cells moved.

### `join` - key-matched added/removed/changed

Rows are matched by the key column(s) you name with `--diff-on`
(matched by **name**, so column order is irrelevant). Keys present on
only one side are tagged `only_in_a` / `only_in_b`; matched keys whose
non-key cells differ are tagged `changed`, again with a
`changed_columns` field. This is the database-style diff: "which `id`s
were added, removed, or edited".

## Output

A table whose first column, `status`, tags each row, followed by the
data columns. For `ordered` / `join` a `changed_columns` column names
the differing fields on a changed row:

| `status`    | Meaning                                                            |
|-------------|--------------------------------------------------------------------|
| `only_in_a` | The row appears in FILE_A but not FILE_B.                          |
| `only_in_b` | The row appears in FILE_B but not FILE_A.                          |
| `changed`   | (`ordered`/`join`) The matched row differs; see `changed_columns`. |

Unchanged rows are **not** printed. A one-line summary (per-mode counts)
is written to **standard error**, so it does not pollute the parseable
table on standard output.

## Examples

### What changed between two snapshots

```bash
$ octa --diff users_v1.csv users_v2.csv
status     id  name
only_in_a  1   alice
only_in_b  4   dave
shared 2 row(s) - only in A: 1 - only in B: 1
```

(The `shared ...` line above is on stderr.)

### Which cells changed, row for row

```bash
$ octa --diff users_v1.csv users_v2.csv --diff-mode ordered
status   id  name   changed_columns
changed  2   bobby  name
mode ordered - unchanged: 1 - changed: 1 - only in A: 0 - only in B: 0
```

### Added / removed / edited by key

```bash
$ octa --diff users_v1.csv users_v2.csv --diff-mode join --diff-on id
status     id  name   changed_columns
changed    2   bobby  name
only_in_a  1   alice
only_in_b  4   dave
mode join - unchanged: 1 - changed: 1 - only in A: 1 - only in B: 1
```

### Just the JSON, for piping to `jq`

```bash
octa --diff a.parquet b.parquet -f json \
  | jq '[.[] | select(.status == "only_in_b")]'
```

## Exit codes

`--diff` exits `0` on a successful read of both files, regardless of
whether they differ. Non-zero exits map to read failures (file not
found, no reader available).

## See also

- [`octa --compare-schemas`](compare-schemas.md): diff the column
  metadata instead of the rows.
- [MCP `diff_tables`](../mcp/tools/diff_tables.md): the same feature
  over MCP.
- The GUI [Compare view](../usage/view-modes/compare.md) does an
  interactive row / text diff of two open tabs.
