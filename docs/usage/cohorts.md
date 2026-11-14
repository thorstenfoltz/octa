# Cohorts

A cohort table answers "of the customers who first showed up in January, how
many came back in February, March, April?". Each person belongs to the period
of their **first** row, their cohort, and each later period counts how many of
that cohort were active again.

**Analyse > Cohorts...** builds it from any table with a column naming a
person (a customer ID, an email) and a date column.

## The dialog

| Field              | What it does                                                                                         |
|--------------------|------------------------------------------------------------------------------------------------------|
| Who                | The column naming a person or customer.                                                              |
| When               | The date or timestamp of each row. Text in the form `2024-03-15` works too. Date columns come first. |
| Period             | Day, week (starting Monday), month, quarter or year.                                                 |
| Show               | **Share still active** (the default), **Number still active**, or **Sum of a column**.               |
| Sum of             | The number column to add up, such as revenue. Only for Sum of a column.                              |
| Open a heatmap too | Also opens the result as a heatmap chart. On by default.                                             |

**Run** opens the cohort table in a new tab, and the heatmap in a second one
when it is ticked. The source table is never changed.

## An example

Five orders by three customers:

| customer | ordered_at |
|----------|------------|
| ann      | 2024-01-10 |
| ann      | 2024-02-05 |
| bob      | 2024-01-22 |
| bob      | 2024-03-14 |
| cy       | 2024-02-18 |

With **Who** `customer`, **When** `ordered_at`, **Period** month and **Share
still active**, Ann and Bob both first ordered in January, so they are the
`2024-01` cohort of 2. Only Ann ordered again in February (50), only Bob in
March (50). Cy first ordered in February and not in March (0). The data ends
in March, so Cy's cohort has no `month_2` yet. The result is the table below.

## Reading the table

For the example above:

| cohort  | size | month_0 | month_1 | month_2 |
|---------|------|---------|---------|---------|
| 2024-01 | 2    | 100     | 50      | 50      |
| 2024-02 | 1    | 100     | 0       |         |

- **cohort** is the period the people first showed up in, written as
  `2024-01`, `2024-W05`, `2024-Q1`, `2024` or a date.
- **size** is how many people the cohort has.
- **`<period>_0`** is the first period itself, so a share there is always 100.
  `month_1` is one month later, and so on.
- **Share** is the percent of the cohort active in that period, **Number** the
  people, **Sum** the column's total over that cohort's rows in that period.
- **An empty cell is a period the data does not reach yet.** The newest cohort
  has the fewest periods behind it, which gives the table its triangle.

A person counts once per period however many rows they have in it. Rows with
no Who, or a When that does not read as a date, are left out, and the status
line says how many.

## On a database tab

On a database tab that does not hold every row, the cohort table is computed
by the database over **every** row, as long as the When column is a typed
date or timestamp there. Only the small result comes back. A When column
stored as text uses the loaded rows instead, and the dialog says so before you
run it. On a server only NULL counts as empty. See
[Analyses on live databases](analyses-on-live-databases.md#cohorts).

## See also

- Command line: [`--cohort`](../cli/cohort.md)
- MCP: [`cohort_table`](../mcp/tools/cohort_table.md)
- [Chart](chart.md) for the heatmap
