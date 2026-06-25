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
