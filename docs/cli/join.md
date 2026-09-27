# `--join`

Join two or more tabular files on shared key column(s) and print the
matched result to stdout.

```
octa --join FILE --join-file FILE2 [--join-file FILE3 ...] \
     --join-on COL[,COL,...] [--join-type left|inner|right|full|semi|anti|asof] \
     [-f tsv|json|csv]
```

The positional `FILE` plus every `--join-file` value form the input list
(at least two files total). `--join-on` is required.

## Join types

- `left` (default) - keep every row of the first file.
- `inner` - keep only rows whose key exists in all files.
- `right` - keep every row of the last file.
- `full` - keep every row of all files.
- `semi` - keep the rows of the first file that have a partner, first
  file's columns only.
- `anti` - keep the rows of the first file that have **no** partner.
- `asof` - the **last** `--join-on` column is matched to the nearest
  earlier value instead of an equal one (the others still match
  exactly), so a trade gets the last quote before it. Rows without an
  earlier partner are kept with empty columns.

Joins run through DuckDB, so they are fast on large files.

## Examples

```
octa --join orders.parquet --join-file customers.parquet --join-on customer_id
octa --join a.csv --join-file b.csv --join-on id,date --join-type inner -f csv
octa --join customers.csv --join-file orders.csv --join-on customer_id --join-type anti
octa --join trades.csv --join-file quotes.csv --join-on ticker,time --join-type asof
```

## See also

- [`--union`](union.md): stack files vertically instead.
- [Join Tables](../usage/join-tables.md) (GUI) and the
  [`join_tables`](../mcp/tools/join_tables.md) MCP tool.
