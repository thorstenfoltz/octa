# Calculated columns

**Columns > Calculated column...** adds one column that holds a rank, a
running total, a share of the total, the change to the previous row, a
neighbouring row's value, a visit number or an ABC class. It is
the quick way to answer "who is first in each region?" or "how much have we
sold so far?" without writing a formula or a SQL window function.

The new column is inserted beside the value column, or at the left edge for
Row number, which has no value column. **Edit > Undo** removes it again.

## The eleven kinds

| Kind                               | What the new column holds                                                                         |
|------------------------------------|---------------------------------------------------------------------------------------------------|
| **Rank**                           | The place of each row by its value. Ties share a place and leave a gap (1, 2, 2, 4).              |
| **Dense rank**                     | Like Rank, but ties leave no gap (1, 2, 2, 3).                                                    |
| **Row number**                     | 1, 2, 3 in the chosen order. Ties keep the table order. Needs no value column.                    |
| **Running total**                  | The sum of the value up to and including each row, in the chosen order.                           |
| **Percent of total**               | Each value as a percentage of its group's total. 12.5 means 12.5 percent.                         |
| **Difference to previous row**     | The value minus the previous row's value, in the chosen order.                                    |
| **Percent change to previous row** | How much the value changed from the previous row, in percent. Empty when the previous value is 0. |
| **Value from previous row**        | The value a number of rows before, in the chosen order. Empty at the start of a group.            |
| **Value from next row**            | The value a number of rows after, in the chosen order. Empty at the end of a group.               |
| **Visit number from timestamps**   | 1, 2, 3 per user: a new visit starts after a pause longer than the gap. See below.                |
| **ABC class**                      | A, B or C by the running share of the total, largest value first. See below.                      |

Running total, Percent of total, Difference, Percent change and ABC class do
arithmetic, so the value column must hold numbers. Numbers stored as text are read as
numbers and blank cells are allowed; text that does not read as a number is
refused.

## Options

| Option                                   | What it does                                                                                                                                                                                                |
|------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Kind**                                 | One of the eleven kinds above.                                                                                                                                                                              |
| **Value column**                         | The column the calculation works on. Not used by Row number.                                                                                                                                                |
| **Order by**                             | The row order for Row number, Running total, Difference and Percent change. **(table order)** keeps the rows as they are. Rank and Dense rank order by the value itself, and Percent of total has no order. |
| **Largest first**                        | Reverses the order, largest or latest value first. On by default for Rank and Dense rank, so the largest value gets 1.                                                                                      |
| **Restart for each**                     | Columns whose values restart the calculation, for example per region. Any number of columns.                                                                                                                |
| **Rows away**                            | For the previous and next row's value: how many rows before or after. Empty means 1.                                                                                                                        |
| **Gap in minutes**                       | For the visit number: a pause longer than this starts a new visit. Empty means 30.                                                                                                                          |
| **A up to percent**, **B up to percent** | For the ABC class: where class A and class B end, as a running share of the total. Empty means 80 and 95, the classic 80 / 15 / 5 split.                                                                    |
| **New column name**                      | The name of the result. It must be free: the default never clashes, and a name you type that is taken is refused.                                                                                           |

The **Preview** shows the first rows with the new column.

## A worked example

Take this table, in this row order, with **Restart for each** set to
`region` and **Order by** left at table order:

| region | amount | Rank | Running total |
|--------|--------|------|---------------|
| North  | 30     | 2    | 30            |
| North  | 50     | 1    | 80            |
| North  | 20     | 3    | 100           |
| South  | 40     | 1    | 40            |
| South  | 10     | 2    | 50            |

Rank counts the largest amount as 1 within each region. The running total adds
up row by row and starts again at the first South row. Without a group, the
whole table counts as one group.

## Value from the previous or next row

The new column repeats the value column, shifted. With **Order by** set to a
date and **Restart for each** set to a customer, **Value from previous row**
puts each customer's previous order beside the current one, and **Value from
next row** the following one. The result keeps the value column's type, so a
date stays a date. **Rows away** reaches further: 12 on monthly rows is the
same month a year earlier. **Order by** and **Largest first** apply as for the
two changes.

| customer | ordered_at | amount | Previous amount |
|----------|------------|--------|-----------------|
| ann      | 2024-01-10 | 20     |                 |
| ann      | 2024-02-05 | 35     | 20              |
| ann      | 2024-03-02 | 15     | 35              |
| bob      | 2024-01-22 | 50     |                 |
| bob      | 2024-03-14 | 40     | 50              |

Each customer's first order has nothing before it, so it stays empty. Without
**Restart for each**, Bob's first row would show Ann's 20, the order before it
by date.

## Visit numbers from timestamps

Pick the column that holds the date and time as **Value column**, the user or
device column under **Restart for each**, and the kind **Visit number from
timestamps**. Each user's rows are put in time order and numbered: the first
row is visit 1, and the number goes up by one whenever the pause since that
user's previous row is longer than the **Gap in minutes**.

| user | at               | Visit |
|------|------------------|-------|
| ann  | 2026-03-01 10:00 | 1     |
| ann  | 2026-03-01 10:20 | 1     |
| ann  | 2026-03-01 11:30 | 2     |
| ben  | 2026-03-01 10:05 | 1     |

A pause of exactly the gap still belongs to the same visit. The rows need not
be sorted, and the table's row order does not change. A date without a time
counts as midnight, a plain number counts as seconds, and text that is no date
or time is refused. A row without a timestamp gets no number.

## ABC classes

The rows are sorted by the value, largest first, and each gets a class by the
share of the total that the rows before it already cover: **A** while that
share is below 80 percent, **B** below 95 percent, **C** after that. So the
few rows that make up most of the total are the A rows. The row that crosses a
limit still belongs to the class it started in, which makes the largest row
always an A. Type other limits into **A up to percent** and **B up to
percent** when the classic 80 / 15 / 5 split does not fit; A must be below B
and both between 0 and 100.

| product | revenue | share before | ABC class |
|---------|---------|--------------|-----------|
| P1      | 50      | 0            | A         |
| P2      | 30      | 50           | A         |
| P3      | 10      | 80           | B         |
| P4      | 5       | 90           | B         |
| P5      | 4       | 95           | C         |
| P6      | 1       | 99           | C         |

A negative value is refused, since a share of a total with negative parts
means nothing. With **Restart for each**, every group gets its own total and
its own classes. To see the same thing as a picture, use the **Pareto** kind
of the Chart.

## Empty values

An empty cell is a null or blank text.

- **Rank**, **Dense rank** and **Row number** put empty values last.
- **Running total** adds nothing for an empty value and carries on.
- **Difference** and **Percent change** are empty when either side is empty,
  and at the start of each group.
- **Percent of total** leaves an empty cell empty. It is also empty when its
  group's total is 0.
- **Value from previous row** and **Value from next row** hand an empty cell
  on as it is.
- **Visit number** and **ABC class** leave a row with an empty value empty and
  do not count it. ABC classes are also empty when the group's total is 0.

## Recipes

The column is recorded in a [recipe](recipes.md) as a "calculated" step, by column
name, so it can be replayed on another file with the same columns.

## On a live database

On a database tab that holds only part of its table, the database computes the
column over every row, with window SQL, and the table is read again with it.
Because the server changed the data, **Undo** cannot take that back. Save or
undo your changes in the tab first.

- Only NULL counts as empty on a server, so blank text ranks as a value there.
- Text ranks follow the database's collation, which is case-blind on MySQL and
  SQL Server. A file compares text exactly.
- ClickHouse has no suitable window function for the two changes and the
  neighbouring row's value, so those four kinds are computed on the loaded
  rows there.
- A database has no row order of its own, so Row number, Running total,
  Difference, Percent change and the previous or next row's value need an
  **Order by** column on a server.
  Without one they are computed on the loaded rows only, and the status line
  says so.
- A visit number is always computed on the loaded rows, with the note.
- An ABC class is computed by the server, which does not refuse negative
  values as a file does. Rows with the same value may fall on either side of
  a limit there.
- If the spec uses a column the server added itself, or the engine cannot
  express it, the column is also computed on the loaded rows with a note.

More on this is on
[Analyses on live databases](analyses-on-live-databases.md#calculated-columns).

## Command line and AI agents

- Command line: [`--calculate`](../cli/calculate.md)
- MCP: [`calculate_column`](../mcp/tools/calculate_column.md), which the
  in-app assistant uses too.
