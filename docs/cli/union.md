# `--union`

Stack two or more tabular files into one output table, reconciling
differing schemas, and print the result to stdout.

```
octa --union FILE --union-file FILE2 [--union-file FILE3 ...] \
     [--union-drop COL]... [--union-cast COL=TYPE]... [-f tsv|json|csv]
```

The positional `FILE` plus every `--union-file` value form the input list
(at least two files total).

## How it works

The result has the union of all input columns. Columns missing from a file
are filled with empty cells for its rows. Mixed numeric types widen to a
common number type; otherwise the column falls back to text.

- `--union-drop COL` omits a column from the output (repeatable).
- `--union-cast COL=TYPE` overrides a column's target Arrow type, e.g.
  `--union-cast amount=Float64` (repeatable).

## Examples

```
octa --union jan.csv --union-file feb.csv --union-file mar.csv -f csv > q1.csv
octa --union a.parquet --union-file b.parquet --union-drop notes
```

## See also

- [`--join`](join.md): match rows on a key instead of stacking them.
- [Union Tables](../usage/union-tables.md) (GUI) and the
  [`union_tables`](../mcp/tools/union_tables.md) MCP tool.
