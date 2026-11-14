# Funnel

A funnel answers "of the people who viewed a product, how many put it in the
cart, and how many of those paid?". You name the steps in order, and Octa
counts how many people got through each one and where they dropped out.

**Analyse > Funnel...** builds it from any table with a column naming a
person (a customer ID, an email) and a column saying what happened in each
row. A date column is optional.

## The dialog

| Field          | What it does                                                                                                                                                   |
|----------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Who            | The column naming a person or customer.                                                                                                                        |
| Event          | The column that says what happened in each row, such as `view`, `cart`, `pay`.                                                                                 |
| When           | The date or timestamp of each row. With **(row order)** the rows count in the order of the table.                                                              |
| Within days    | Count a step only when it happens this many days or fewer after the first step. Empty means no limit. Needs a When column.                                     |
| Steps in order | The values of the Event column. Tick the ones that make up the funnel. Ticked steps move to the top, numbered, and the `^` and `v` buttons change their order. |

A funnel has two steps or more, as many as you like, and each event can be a
step once. The list offers the first 1,000 different values of the Event
column.

**Run** opens the result in a new tab. The source table is never changed.

## An example

Seven events by three people:

| user | event | at         |
|------|-------|------------|
| ann  | view  | 2024-05-01 |
| ann  | cart  | 2024-05-01 |
| ann  | pay   | 2024-05-02 |
| bob  | view  | 2024-05-01 |
| bob  | cart  | 2024-05-03 |
| cy   | view  | 2024-05-02 |
| cy   | pay   | 2024-05-02 |

With **Who** `user`, **Event** `event`, **When** `at` and the steps `view`,
`cart`, `pay` in that order, all three viewed, Ann and Bob put something in
the cart, and only Ann paid after that. Cy paid without a cart, so Cy stops
at the first step: a step only counts after the one before it. The result is
the table below.

## Reading the table

For the example above:

| step | users | share_of_first | share_of_previous | dropped |
|------|-------|----------------|-------------------|---------|
| view | 3     | 100            | 100               | 0       |
| cart | 2     | 66.7           | 66.7              | 1       |
| pay  | 1     | 33.3           | 50                | 1       |

- **users** is how many people reached the step, having done every step
  before it, in order.
- **share_of_first** is the percent of the people who did the first step.
- **share_of_previous** is the percent of the people who reached the step
  before.
- **dropped** is how many people reached the step before but not this one.

## How a step counts

A person reaches a step when they have that event **at or after** the moment
they reached the step before. So a payment made before the cart does not
count as the third step, and a person who never did the first step is not in
the funnel at all, whatever else they did. Two events at the same moment
count in either order. A person counts once per step however many rows they
have.

With **Within days**, the clock starts at a person's first event of the first
step: a later step counts only inside that many days from then.

Rows with no Who, or a When that does not read as a date, are left out, and
the status line says how many.

## On a database tab

On a database tab that does not hold every row, the funnel is counted by the
database over **every** row, as long as the When column is a typed date or
timestamp there. Only the counts come back. Without a When column, or with
one stored as text, the loaded rows are used instead, and the dialog says so
before you run it. The step list always shows the events of the loaded rows.
On a server only NULL counts as empty, and step names compare by the server's
rules, so `View` and `view` are one step on a case-blind engine such as MySQL
or SQL Server. See
[Analyses on live databases](analyses-on-live-databases.md#funnel).

## See also

- Command line: [`--funnel`](../cli/funnel.md)
- MCP: [`funnel`](../mcp/tools/funnel.md)
- [Cohorts](cohorts.md) for who came back, per period
