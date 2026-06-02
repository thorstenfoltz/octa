# `octa --tail`

Print the last N rows of a file, the way Unix `tail` does for text
files, except Octa understands the binary formats too. Default
20 rows.

## Synopsis

```bash
octa --tail FILE [-n N] [-f tsv|json|csv] [--rows N|all]
```

| Flag                | Default | Meaning                                                         |
|---------------------|---------|-----------------------------------------------------------------|
| `-n N`, `--lines N` | `20`    | Number of trailing rows to print. Must be ≥ 0.                  |
| `-f`, `--format`    | `tsv`   | Output format (see [CLI overview](index.md#output-formatting)). |
| `--rows N\|all`     | 5 M     | Initial-load row cap for streaming formats (see Performance).   |

## Examples

### Default: last 20 rows as TSV

```bash
octa --tail sales.parquet
```

### Custom row count

```bash
octa --tail sales.csv -n 5             # last 5 rows
octa --tail sales.csv -n 1             # just the final row
```

### JSON output for downstream tools

```bash
octa --tail sales.parquet -n 3 -f json
```

## Performance

For streaming formats (Parquet, CSV, TSV), Octa loads the standard
**initial-load row cap** (5 million rows by default), then keeps the
last N rows of that loaded window. This means that on a file larger
than the cap, `--tail` reflects the end of the *loaded window*, not
necessarily the true end of the file. To tail the genuine end of a
very large file, raise the cap:

```bash
octa --tail huge.parquet -n 20 --rows all
```

For non-streaming formats (Excel, SQLite, JSON, etc.) the whole table
is loaded into memory and the last N rows are sliced off.

## Notes

- `-n 0` prints just the header row.
- For multi-table sources (SQLite, DuckDB), Octa loads the first
  table.
- Like `--head`, TAB and newline characters in cells are replaced with
  spaces in TSV output; use `-f csv` or `-f json` for lossless output.

## See also

- [`octa --head`](head.md): the first N rows.
- [`octa --sample`](sample.md): a reproducible random sample.
- [MCP `tail` tool](../mcp/tools/tail.md): the same access pattern via
  MCP.
