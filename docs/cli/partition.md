# `--partition-by`

Split a file into one output file per distinct value of a column, written
into a directory. The input file is never modified.

```
octa --partition-by COL --out-dir DIR FILE [--partition-format EXT]
```

`--out-dir` is required (created if absent). `--partition-format` sets the
output extension without the dot (e.g. `csv`, `parquet`); it defaults to the
source file's extension.

## How it works

Octa writes one file per distinct value of `COL`, named after the value.
Partitioning a sales table by `region` produces `North.csv`, `South.csv`,
and so on. A one-line summary is printed to stderr.

## Examples

```
octa --partition-by region --out-dir ./by_region sales.csv
octa --partition-by year --out-dir ./by_year sales.parquet --partition-format parquet
```

## See also

- [Partition by Column](../usage/partition-by-column.md) (GUI) and the
  [`partition_table`](../mcp/tools/partition_table.md) MCP tool.
