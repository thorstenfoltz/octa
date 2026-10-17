# Split into Files

**Data > Split into files...** (Ctrl+Shift+Z) writes the active table as
several files: one per value of a column, at most so many rows each, at most
so much size each, or a mix.

## The three limits

Choose at least one of these; they combine.

- **Column**: one file per distinct value, named after the value. Splitting a
  sales table by `region` gives `north.csv`, `south.csv`, and so on. Leave it
  at **(none)** to split the whole table by rows or size only.
- **Max rows per file**: no file holds more rows than this. Type a whole
  number; `10,000` works.
- **Max size per file (MB)**: no file is bigger than this (1 MB = 1,000,000
  bytes; decimals such as `2.5` work). Octa writes a file, measures it and
  rewrites it with fewer rows if it came out too big, so every format works,
  compressed Parquet included. Files land a little under the limit rather than
  packed to the byte.

With a column and a limit, each value is split further. With only a limit the
files are named after the source file.

## Names

Without a row or size limit the names are the plain ones in the table below.
With a limit every file gets a part number: flat files become
`berlin_part_0001.csv`, `berlin_part_0002.csv`, ..., and the folder layouts
number the files inside their folder (`city=Berlin/part-0001.csv`). The
numbers count per folder, so each folder starts at `part-0001`. The dialog
previews the first file of the first few values with the same naming code the
writer uses.

## Big tabs

If the tab holds only part of its source, the dialog says so and offers
**Load the rest**; a split writes only the rows that are loaded. A tab that
shows one page of a very large file is refused with a pointer to
`octa --split-rows` and `octa --split-size`, which stream the whole file.

## Running

The split runs in the background with a progress line and **Cancel**. Cancel
stops after the file being written; files already written stay. The original
table is not changed.

## Command line and MCP

Also available as `octa --partition-by`, `--split-rows` and `--split-size`
(see the [`--partition-by`](../cli/partition.md) reference) and as the
[`partition_table`](../mcp/tools/partition_table.md) MCP / assistant tool.

### Choosing how the pieces are named

The **Layout** option offers four shapes. Taking `city` with values
`New York`, `Berlin` and `Sao Paulo`, and writing CSV:

| Layout                           | On disk                                               |
|----------------------------------|-------------------------------------------------------|
| **Flat files**                   | `new_york.csv`, `berlin.csv`, `sao_paulo.csv`         |
| **Folder per value**             | `New York/part-0001.csv`, `Berlin/part-0001.csv`, ... |
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
