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

## Layouts

`--partition-layout` takes four words. With `city` holding `New York`,
`Berlin` and `Sao Paulo`:

```bash
octa --partition-by city --out-dir ./by-city --partition-layout hive sales.csv
```

| Word             | On disk                                                    |
|------------------|------------------------------------------------------------|
| `flat` (default) | `new_york.csv`, `berlin.csv`, `sao_paulo.csv`              |
| `folder`         | `New York/part-0001.csv`, `Berlin/part-0002.csv`           |
| `hive`           | `city=New York/data.csv`, `city=Berlin/data.csv`           |
| `hive-parts`     | `city=New York/part-0001.csv`, `city=Berlin/part-0002.csv` |

**All four hold the same rows**, and all four can be reopened as one table
with **File > Open table folder...**, because the partition column is written
into every file whichever you pick. Only the names differ - and only `flat`
loses information in them: it folds the value into a SQL-safe stem, so
`New York`, `new-york` and `NEW_YORK` all become `new_york`, with `_2` and
`_3` appended to disambiguate. The three folder layouts keep the value as it
is, replacing only characters that cannot appear in a path.

`hive` is what Spark, Athena, DuckDB and pandas expect from a partitioned
dataset; the `-parts` variants use the `part-0001` file naming those tools
write themselves.
