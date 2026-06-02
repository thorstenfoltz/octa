# `octa --diff`

Row-level diff of two files: print the rows present in only one side.
Where [`--compare-schemas`](compare-schemas.md) diffs the column
*metadata*, `--diff` diffs the actual *rows*.

## Synopsis

```bash
octa --diff FILE_A FILE_B [-f FORMAT]
```

| Flag                    | Required | Meaning                                           |
|-------------------------|----------|---------------------------------------------------|
| `--diff A B`            | yes      | The two files to compare. Exactly two paths.      |
| `-f`, `--format FORMAT` | no       | Output format: `tsv` (default), `json`, or `csv`. |

## How rows are compared

Each row is keyed by its **whole-row content**: every column, in order,
rendered to text and joined. A row in A matches a row in B when their
keys are equal. Two consequences follow:

- **Columns are positional.** The two files should share the same
  column order (and, ideally, names) for the diff to be meaningful.
  A reordered column set will report everything as changed.
- **Cross-format works.** Because matching is on rendered values, a CSV
  row and a Parquet row with the same logical content match. You can
  diff `before.csv` against `after.parquet`.

## Output

A table whose first column, `status`, tags each row, followed by the
data columns:

| `status`    | Meaning                                   |
|-------------|-------------------------------------------|
| `only_in_a` | The row appears in FILE_A but not FILE_B. |
| `only_in_b` | The row appears in FILE_B but not FILE_A. |

Rows present in both files are **not** printed. A one-line summary
(`shared`, `only in A`, `only in B` counts) is written to **standard
error**, so it does not pollute the parseable table on standard output.

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
