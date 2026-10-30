# Times and durations

Octa has two cell types for time that has no date attached:

- **Time** is a clock time without a date: `14:30:00`, `09:05:00.25`. It
  runs from `00:00:00` to `23:59:59.999999999`. A shift that starts at
  `22:00` is a Time.
- **Duration** is an amount of time: `08:30:00`, `30:15:00`, `-00:30:00`.
  It can be negative and can be longer than a day. How long a shift lasted
  is a Duration.

Both sort, compare, chart and convert like numbers, so `clock_in > 09:00`
and the sum of a `worked` column mean what you expect. A date with a time
of day (`2024-05-01 14:30:00`) stays a datetime; these two types are for
the clock reading or the amount alone.

## How a column becomes one

**When a file opens.** A text column whose every filled value is a clock
reading becomes a Time column. A column whose values are amounts of time
(at least one negative, 24 hours or more, written with days, or in the
ISO form) becomes a Duration column. Anything else stays text, and so does
a column with no values. This applies to the text-based formats (CSV, TSV,
JSON, JSON Lines, YAML, XML, fixed-width, logs, HTML and PDF tables,
MessagePack, BSON, ODS); the binary formats and databases already say
which columns are times.

Turn it off under **Settings > Opening files > Detect times and durations
on open** to keep such columns as text. It is on by default.

**Change column type.** **Columns > Change column type...** offers
**Time of day** and **Duration**. Its preview counts the values that
convert; one that does not read as a time keeps its text. **Insert
column** offers the same two types.

What Octa reads as a clock time: `14:30`, `14:30:00`, `9:05`,
`14:30:00.250`, `9:05 AM`, `11:45 PM` (AM and PM in either case). What it
reads as a duration: `30:15:00`, `-01:30:00`, `1 day 06:15:00`, `2 days`,
`PT30H15M`, `P1DT2H`, and the interval text Postgres, DuckDB, BigQuery,
Trino and Oracle print.

What stays text, on purpose:

- `24:00:00` and the leap second `23:59:60`: neither is a clock reading
  Octa can store.
- A time with a zone (`TIMETZ`, `TIME WITH TIME ZONE`).
- An interval with a month or year part (`1 mon 2 days`): a month has no
  fixed length. One such value turns its whole column into text, so no
  later save has to drop it.
- A Postgres interval stored as more than 24 hours without days
  (`51:00:00`) or with mixed signs (`-1 days +02:00:00`). Postgres keeps an
  interval's days apart from its hours, and the two differ across a clock
  change. Octa writes a duration back as whole days plus the rest, so these
  values stay as Postgres printed them and a save cannot change them.

## How a duration is shown

**Settings > Table View > Duration format** picks one of three spellings.
The same spelling is used when Octa writes CSV and other text files, and
Octa reads all three back.

| Format   | Example          | Notes                                   |
|----------|------------------|-----------------------------------------|
| Clock    | `30:15:00`       | The default. Hours go past 24.          |
| Days     | `1 day 06:15:00` | Postgres and DuckDB read this form too. |
| ISO 8601 | `PT30H15M`       | The form many programs exchange.        |

Chart axes always use the clock style, since an axis needs short labels.

## Editing a cell

Type a new value into a Time or Duration cell in any of the spellings
above. Text that does not read as one is kept as you typed it, and the
cell shows as text until you fix it.

## Saving

| Format                         | Time                   | Duration                                             |
|--------------------------------|------------------------|------------------------------------------------------|
| Parquet, Arrow IPC, Feather    | `Time64`               | `Duration`, or whole microseconds (see below)        |
| DuckDB file, SQL panel         | `TIME`                 | `INTERVAL`                                           |
| Excel (`.xlsx`)                | time value, `hh:mm:ss` | time value, `[h]:mm:ss`; a negative duration as text |
| ODS                            | time value             | duration value, negatives included                   |
| Avro                           | `time-micros`          | text                                                 |
| TOML                           | local time             | text                                                 |
| CSV, TSV, JSON, YAML, XML, ... | the shown text         | the shown text in the chosen format                  |
| SQLite, ORC                    | the shown text         | the shown text                                       |

**Parquet and durations.** A Duration column is written as an Arrow
`Duration`. pyarrow, polars and Octa read it back as a duration; DuckDB and
Spark see a plain integer. **Settings > Saving files > Write options > Parquet >
Durations as integer microseconds** writes whole microseconds instead, which every
tool reads the same way. Save As to Parquet shows a one-line note when the
table has a duration column and this switch is off.

A column stores microseconds unless one of its values has digits below a
microsecond, which makes it a nanosecond column. Digits below the column's
unit are cut on save.

## Databases

Octa reads these server types as Time and Duration:

| Database             | Time                                   | Duration                               |
|----------------------|----------------------------------------|----------------------------------------|
| Postgres, Redshift   | `time`                                 | `interval` without months (see above)  |
| MySQL, MariaDB       | (none)                                 | `TIME`, from -838:59:59 to 838:59:59   |
| SQL Server           | `time` (100 ns precision kept)         | (no interval type)                     |
| Oracle               | (no TIME type)                         | intervals stay text                    |
| Snowflake            | `TIME`                                 | (no interval type)                     |
| BigQuery             | `TIME`                                 | `INTERVAL` without months              |
| Databricks           | (no TIME type)                         | `INTERVAL DAY TO SECOND` and its parts |
| Trino, Athena        | `time`, `time(p)`                      | `interval day to second`               |
| Exasol               | (no TIME type)                         | `INTERVAL DAY TO SECOND`               |
| ClickHouse           | `Time`, `Time64`                       | (no interval type)                     |
| DuckDB, SQLite files | `TIME`; SQLite: a column declared TIME | `INTERVAL` without months              |

MySQL's `TIME` is a duration (MySQL's own definition: it reaches 838
hours), so it reads as one.

When Octa creates a table (**Save to database**, a copy between servers,
the CLI and MCP write tools) it uses these column types:

| Database           | Time           | Duration                       |
|--------------------|----------------|--------------------------------|
| Postgres, BigQuery | `TIME`         | `INTERVAL`                     |
| Redshift           | `TIME`         | `VARCHAR(64)`                  |
| MySQL, MariaDB     | `TIME(6)`      | `TIME(6)`                      |
| SQL Server         | `TIME(7)`      | `NVARCHAR(64)`                 |
| Oracle             | `VARCHAR2(32)` | `INTERVAL DAY(9) TO SECOND(9)` |
| Snowflake          | `TIME(9)`      | `VARCHAR`                      |
| Databricks         | `STRING`       | `INTERVAL DAY TO SECOND`       |
| Trino, Athena      | `TIME(6)`      | `VARCHAR`                      |
| Exasol             | `VARCHAR(32)`  | `INTERVAL DAY(9) TO SECOND(9)` |
| ClickHouse         | `String`       | `String`                       |
| SQLite             | `TEXT`         | `TEXT`                         |

A text column gets the value in the clock spelling (`14:30:00`,
`30:15:00`), whatever the display setting, because the server has to read
it.

**MySQL's range.** MySQL cuts a `TIME` outside -838:59:59 to 838:59:59
short without a word. Octa refuses to send such a value instead: the save
stops before anything is written, and the message names the column and
the value.

## Analysis

- **[Date / Time calculation](date-time-calculation.md)** has four
  operations for these types: the duration between two times or dates
  (with **End before start means next day** for night shifts), adding a
  duration to a time or a date, and turning a duration into a number of
  hours, minutes or seconds and back. **Extract a component** pulls the
  hour, minute or second out of a Time.
- **Summary** gives a Time column its minimum, maximum and quartiles (real
  values of the column) and no average: clock times have no meaningful
  mean. A Duration column also gets its sum and mean.
- **Data drift** compares a Duration column's minimum, maximum and mean;
  **Compare distributions** tests two Time or two Duration columns like
  numbers; **Detect outliers** checks Duration columns (not Time, which
  wraps at midnight); **Value frequency** bins both kinds with labels such
  as `[06:00:00, 08:00:00)`.
- **[Charts](chart.md)**: Line, Scatter, Histogram and Box plot
  them as numbers with clock labels. A Time X axis spans 00:00 to 24:00
  unless you set the bounds.
- **[Pivot](pivot.md)** and **Time series** sum and average Duration
  columns, and offer only count, minimum, maximum, first and last for a
  Time column. On a database tab, a sum or average of a time column runs
  on the loaded rows and the result tab says so.
- **Comparisons** read times: a conditional formatting rule or an Ask
  filter `clock_in > 08:30` compares clock times, `worked < 1 day` compares
  durations, and `08:30` equals `08:30:00`. A **Data validation** Range on
  a time column takes its bounds as times (`08:00` to `18:00`). The
  **Compare** tolerance in seconds covers both types.

## Command line and MCP

The command line and the MCP server read the same settings (detection,
duration format, the Parquet switch), so a file written headless is the
file the window would write. `--schema` and the `schema` tool name the
types `Time64(Microsecond)` and `Duration(Microsecond)`.

Wherever a type is named, `time` and `duration` work: casts in
`transform_columns` and recipe **Change type** steps, column specs in
`write_table`, `--union-cast`, and JSON Schemas for `--validate-schema`
(`"format": "time"` / `"format": "duration"`). A Time column read in
microseconds and one read in nanoseconds count as the same type in schema
comparisons, schema drift and harmonise.

## See also

- [Date / Time calculation](date-time-calculation.md)
- [Change column type](change-column-type.md)
- [Settings](../reference/settings.md)
