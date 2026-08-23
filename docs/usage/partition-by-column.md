# Partition by Column

**Data > Partition by column...** (Ctrl+Shift+Z) splits the active table
into one file per distinct value of a column, like sorting rows into folders
by category.

## How it works

Pick the column to split on and an output folder, then choose the output
format. Octa writes one file per distinct value (named after the value).
For example, partitioning a sales table by `region` produces `North.csv`,
`South.csv`, and so on. The original table is not changed.

## Command line and assistant

Also available as `octa --partition-by` (see the
[`--partition-by`](../cli/partition.md) reference) and as the
[`partition_table`](../mcp/tools/partition_table.md) MCP / assistant tool.

### Choosing how the pieces are named

The **Layout** option offers four shapes. Taking `city` with values
`New York`, `Berlin` and `Sao Paulo`, and writing CSV:

| Layout                           | On disk                                               |
|----------------------------------|-------------------------------------------------------|
| **Flat files**                   | `new_york.csv`, `berlin.csv`, `sao_paulo.csv`         |
| **Folder per value**             | `New York/part-0001.csv`, `Berlin/part-0002.csv`, ... |
| **Hive folders**                 | `city=New York/data.csv`, `city=Berlin/data.csv`, ... |
| **Hive folders, numbered files** | `city=New York/part-0001.csv`, ...                    |

**All four hold the same rows**, and all four reopen as one table with
**File > Open table folder...**, because the column you split on is written
into every file whichever you pick. The choice is only about the names - but
the names are not merely cosmetic:

- **Flat** is the only lossy one. The value goes through the same tidy-up a
  SQL identifier gets: capitals fold to lower case and anything that is not a
  letter or digit becomes an underscore. So `New York`, `new-york` and
  `NEW_YORK` all arrive as `new_york`, and the second and third become
  `new_york_2.csv` and `new_york_3.csv`. No rows are lost, but the name no
  longer tells you which value is inside. An empty value becomes `table.csv`
  and one starting with a digit gains a `t_` prefix.
- **The three folder layouts keep the value intact**, replacing only
  characters that cannot appear in a path at all.
- **Hive** (`column=value`) is what Spark, Athena, DuckDB and pandas expect
  from a partitioned dataset. Choose it when the files are going into another
  tool.
- **The numbered variants** name the file `part-0001` instead of after the
  value or `data`, matching what those tools write themselves. Some pipelines
  expect that shape.

You do not have to work any of this out from the descriptions: the dialog
shows a **live preview** of the first few paths, built from your own column's
values with the same code that writes the files. Change the column, the
format or the layout and the preview follows.

Flat is the default, so nothing changes unless you pick otherwise.
