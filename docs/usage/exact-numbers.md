# Exact numbers

Octa keeps every digit of a number and adds numbers up without rounding.
`12345678901234567890.12` keeps all its digits, and 0.1 + 0.2 + 0.3 is 0.6,
not 0.6000000000000001.

## Reading text files

In a text file (CSV, TSV, JSON, YAML, XML, fixed width, logs, HTML, ODS) a
column of numbers with a fraction is read as an exact decimal, as wide as its
longest value. **Change column type** shows it as `Decimal128(p, s)`, SQL and
**Summary** as `DECIMAL(p,s)`: `p` digits in all, `s` of them after the point. Whole
numbers stay whole numbers (`Int64`). European numbers such as `1.234,56` are
read the same way once Octa has settled on the column's convention.

Large-file mode and `--sql --stream` read such columns as DuckDB `DECIMAL` too.
Working that out takes one extra pass over the file, which also counts its rows.

A column stays a floating-point number when:

- a value is written with an exponent, such as `1.5e3`
- a value is `NaN` or `inf`
- the column needs more than 38 digits; when such a value would lose digits as
  a float, the column stays text instead, so nothing is lost

Typed files (Parquet, Arrow, Excel, SQLite) keep the type they declare.

Zeros at the end of a fraction are not kept: `1.50` is the number 1.5 and
shows as `1.5`, as in most tools. The value is what counts, so in a number
column `1`, `1.0` and `1.00` are the same number to sorting, sums,
**Find duplicates**, **Drop duplicates** and **Unique columns**. In a text
column they are different text.

## Databases

Decimal columns of a live connection (`DECIMAL`, `NUMERIC`, Oracle `NUMBER`,
BigQuery `NUMERIC`, Snowflake `NUMBER(p, s)`) open as exact decimals on every
engine, every digit kept (`99.50` reads as 99.5, like any number). A column of
whole numbers opens as whole numbers. Saving a row writes every digit back.

Two columns still pass through a floating-point number in the database
driver, so they are exact to about 15 digits:

- SQL Server `MONEY` and `SMALLMONEY`
- Exasol `DECIMAL` with 18 digits or fewer (a wider one arrives exact)

A PostgreSQL `NUMERIC` column holding `NaN` opens as text, every value as the
server wrote it.

## Adding up

Every sum Octa calculates adds each value by the digits it shows:

- the selection summary in the status bar
- **Summary**
- **Pivot**, **Time series** (buckets and rolling windows), **Cohorts**
- chart Sum
- **Calculated columns**: running total and difference to the previous row
- SQL on a text file's decimal columns, also your own queries

A floating-point column, such as a Parquet `DOUBLE` or an Excel column, is
added by the digits it shows in all of these too, so a column that reads 0.1,
0.2 and 0.3 sums to 0.6.

Two limits:

- A query you write yourself on a floating-point column uses DuckDB's float
  arithmetic. Write `sum(CAST(price AS DECIMAL(18, 2)))` for an exact sum.
- Past 38 digits, a sum carries on in careful floating point rather than
  failing.

## Averages

An average is a division, and 1 / 3 has no end, so an average is never
exact in every case.

- The status bar's average and a chart's Average divide the exact sum and
  keep 12 digits after the values' own.
- The average in **Summary**, **Pivot** (Avg) and **Time series** (Mean)
  comes from DuckDB's `avg`: a floating-point number, exact to about 15
  significant digits. The average of 0.1, 0.2 and 0.3 is 0.2; of 0.1, 0.2
  and 0.4 it is 0.23333333333333334. The same holds for `avg()` in your own
  SQL, the CLI's `--resample` and `--rolling`, and the MCP tools `pivot`,
  `resample_timeseries`, `rolling_window` and `profile`.
- Percent of total and percent change in **Calculated columns** are
  floating-point numbers too.

## On a database tab

On a database tab, **Summary**, **Pivot**, **Time series**, **Cohorts**,
charts and **Calculated columns** run on the database itself (see
[Analyses on live databases](analyses-on-live-databases.md)), so the
database does the arithmetic:

- A decimal column (`DECIMAL`, `NUMERIC`, Oracle `NUMBER`) adds exactly
  there too.
- A whole-number column is added as floating-point numbers, which stay exact
  up to 9,007,199,254,740,992.
- A floating-point column (`FLOAT`, `DOUBLE`, `REAL`) adds as the database
  adds floats, so 0.1 + 0.2 + 0.3 can come out as 0.6000000000000001.
- Averages, standard deviations and percentages are floating-point numbers,
  exact to about 15 significant digits.
- On Oracle the floating-point step is a `NUMBER` instead, so every
  `NUMBER` column adds exactly.

The status bar's selection summary adds the loaded rows in Octa, exactly. A
SQLite connection runs nothing on the database, so its tab adds like a file.

## Saving

| Target               | A decimal column is written as                                                        |
|----------------------|---------------------------------------------------------------------------------------|
| Parquet, Arrow, Avro | a real decimal column                                                                 |
| CSV, TSV, JSON       | its digits                                                                            |
| Excel, ODS           | a number, or text when it has more digits than the program keeps (15)                 |
| ORC                  | a double when every value is exact as one, else text                                  |
| YAML, TOML           | a float, or quoted text when it has more digits than a float holds (see Known limits) |
| A database           | its digits, into a `DECIMAL(p, s)` column when Octa creates the table                 |

Converting a CSV file to Parquet used to store such columns as `DOUBLE`. Since
version 0.22.0 it stores `DECIMAL`, so the digits survive the conversion.

## Known limits

- YAML and TOML numbers are floating point by those formats' own rules
  (TOML's specification makes a float a 64-bit double, YAML's float is an
  approximation). Reading, a number past about 15 digits rounds. Saving, a
  number with more digits than a float holds is written as quoted text, so no
  digit is lost but it is text in the file. To keep such a number exact, quote
  it in the file (`amount: "12345678901234567890.12"`).
- A value typed into a decimal column of a Parquet or Arrow file that has more
  digits after the point than the column allows rounds to the column's scale
  when saved, as databases do.

## See also

- [Change column type](change-column-type.md)
- [Database connections](database-connections.md)
- [Summary](summary.md)
- [Supported formats](../getting-started/supported-formats.md)
