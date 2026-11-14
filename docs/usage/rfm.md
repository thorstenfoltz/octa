# Customer segments (RFM)

RFM sorts your customers by three questions: how **r**ecently did they last
buy, how **f**requently do they buy, and how much **m**oney do they spend? Like
a teacher giving each pupil three marks, Octa gives every customer a mark from
1 to 5 per question, and reads a segment name off the marks: **Best**,
**Loyal**, **Slipping away** and so on.

**Analyse > Customer segments (RFM)...** builds it from any table of orders or
order lines with a customer column, a date and an amount.

## The dialog

| Field         | What it does                                                                                                                                                                                              |
|---------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Customer      | The column naming a customer, such as a customer number or an email.                                                                                                                                      |
| Date          | The date of each purchase: a date or timestamp column, or text in the form `2024-03-15`.                                                                                                                  |
| Amount        | What each row was worth. Empty amounts, or text that is no number, count as 0.                                                                                                                            |
| Order number  | Rows with the same order number count as one order, for tables with one row per order line; a row without an order number is an order of its own. With **(every row is one order)** each row is an order. |
| As of         | The day the segments are made for: purchases after it are left out, so the result is the one you would have had on that day. Empty means the latest date in the data. Any date layout Octa reads works.   |
| Segment names | **Seven plain names** (the default), **Eleven marketing names** or **No names, marks only**.                                                                                                              |

**Run** opens the result in a new tab. The source table is never changed.

## An example

Seven purchases by four customers, every row one order, **As of** left empty
(so the latest date, 2024-03-30, counts as today):

| customer | date       | amount |
|----------|------------|--------|
| bob      | 2024-01-05 | 100    |
| ann      | 2024-02-01 | 5.3    |
| ann      | 2024-03-20 | 10     |
| eve      | 2024-01-10 | 0.1    |
| eve      | 2024-02-01 | 0.2    |
| eve      | 2024-02-12 | 0.3    |
| cy       | 2024-03-30 | 0      |

Bob spent most but bought once and long ago: high monetary mark, lowest
recency, so he is **Slipping away**. Cy bought today for nothing: best
recency, lowest frequency and monetary, so **New**. The result is the table
below.

## Reading the table

One row per customer, the largest amount first:

| customer | last_date  | days_since | orders | amount | recency | frequency | monetary | segment         |
|----------|------------|------------|--------|--------|---------|-----------|----------|-----------------|
| bob      | 2024-01-05 | 85         | 1      | 100    | 1       | 1         | 4        | Slipping away   |
| ann      | 2024-03-20 | 10         | 2      | 15.3   | 3       | 3         | 3        | Needs attention |
| eve      | 2024-02-12 | 47         | 3      | 0.6    | 2       | 4         | 2        | Slipping away   |
| cy       | 2024-03-30 | 0          | 1      | 0      | 4       | 1         | 1        | New             |

- **days_since** counts from the last purchase to the As of day.
- **amount** is added up exactly, so 0.1 + 0.2 + 0.3 is 0.6.
- **recency**, **frequency** and **monetary** are the marks. Hover a column
  header for what it means, and a segment name for what to do about it.

## How the marks are given

For each question the customers are sorted and cut into five equal parts: the
fifth with the fewest days since their last purchase get recency 5, the fifth
with the most orders frequency 5, the fifth who spent most monetary 5.

**Equal values always share a mark**: a mark counts the customers strictly
below a value, so when most customers ordered once, they all get frequency 1,
and the next one up starts higher. With few customers, not every mark appears.

## How the names are read

The name comes from the recency mark and the average of the frequency and
monetary marks (FM), halves rounded up. Seven plain names:

| Recency | FM 4 to 5     | FM 3            | FM 2            | FM 1            |
|---------|---------------|-----------------|-----------------|-----------------|
| 4 to 5  | Best          | Loyal           | Promising       | New             |
| 3       | Loyal         | Needs attention | Needs attention | Needs attention |
| 1 to 2  | Slipping away | Slipping away   | Lost            | Lost            |

The eleven marketing names, the grid most RFM articles use:

| Recency | FM 5            | FM 4            | FM 3                | FM 2                | FM 1             |
|---------|-----------------|-----------------|---------------------|---------------------|------------------|
| 5       | Champions       | Champions       | Potential loyalists | Potential loyalists | Recent customers |
| 4       | Champions       | Champions       | Loyal customers     | Potential loyalists | Promising        |
| 3       | Loyal customers | Loyal customers | Needs attention     | Needs attention     | About to sleep   |
| 2       | Can't lose them | At risk         | At risk             | Hibernating         | Hibernating      |
| 1       | Can't lose them | Can't lose them | At risk             | Lost                | Lost             |

The names stay in English in the table, so a saved file reads the same for
everyone; hovering a name explains it in your language.

## What is left out

Rows with no customer or no readable date are left out, and the status line
says how many. Purchases after the As of day are left out as well; when no
customer bought on or before it, Octa says so. On a tab that holds only part of a file, the result is built
from the loaded rows, and the status line says so.

## On a live database

On a database tab that does not hold every row, the database adds up every
row in one query (each customer's latest date, orders and amount, a decimal
amount exactly) and Octa gives the marks and names, so the result is the same
as on a file. The Date column has to be a typed date or timestamp on the
server; one stored as text uses the loaded rows, and the dialog says so before
you run it. On a server only NULL counts as an empty customer or date.

## Command line and MCP

`octa --rfm` (see [`--rfm`](../cli/rfm.md)) and the
[`rfm_segments`](../mcp/tools/rfm_segments.md) MCP tool give the same table.
