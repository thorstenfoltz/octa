# `--dedupe`

Remove duplicate rows from a file and print the result to stdout. The input
file is never modified.

```
octa --dedupe FILE [--dedupe-on COL[,COL,...]] [--dedupe-keep first|last] \
     [-f tsv|json|csv]
```

## How it works

Without `--dedupe-on` the whole row is the duplicate key. With it, only the
named columns form the key, so rows sharing those values collapse to one.
`--dedupe-keep` (default `first`) chooses which occurrence survives.

A one-line summary (input rows, duplicates removed, output rows) is printed
to stderr. Values are compared as text, so `1` and `1.0` are not equal.

## Examples

```
octa --dedupe people.csv -f csv > unique.csv
octa --dedupe people.csv --dedupe-on email --dedupe-keep last
```

## See also

- [Drop Duplicate Rows](../usage/drop-duplicate-rows.md) (GUI) and the
  [`drop_duplicates`](../mcp/tools/drop_duplicates.md) MCP tool.
