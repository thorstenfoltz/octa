# `--join`

Join two or more tabular files on shared key column(s) and print the
matched result to stdout.

```
octa --join FILE --join-file FILE2 [--join-file FILE3 ...] \
     --join-on COL[,COL,...] [--join-type left|inner|right|full] \
     [-f tsv|json|csv]
```

The positional `FILE` plus every `--join-file` value form the input list
(at least two files total). `--join-on` is required.

## Join types

- `left` (default) - keep every row of the first file.
- `inner` - keep only rows whose key exists in all files.
- `right` - keep every row of the last file.
- `full` - keep every row of all files.

Joins run through DuckDB, so they are fast on large files.

## Examples

```
octa --join orders.parquet --join-file customers.parquet --join-on customer_id
octa --join a.csv --join-file b.csv --join-on id,date --join-type inner -f csv
```

## See also

- [`--union`](union.md): stack files vertically instead.
- [Join Tables](../usage/join-tables.md) (GUI) and the
  [`join_tables`](../mcp/tools/join_tables.md) MCP tool.
