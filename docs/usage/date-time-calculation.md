# Date / Time calculation

Octa can derive a new column from date, datetime, or duration values
without writing a formula or a SQL query. It covers the everyday
date-arithmetic chores: how many days between two columns, add a fixed
number of months to a date, convert a duration from minutes to hours,
pull the weekday out of a timestamp, or turn a Unix epoch timestamp into
a readable date (and back).

Open it from **Data → Date/Time calculation…**. The dialog computes the
result into a brand-new column and leaves the source columns untouched,
the same way [Insert Column](editing.md#inserting-columns) and
[formulas](formulas.md) do.

![Date/Time calculation dialog](../assets/screenshots/date-time-calculation.png)

## Operations

Pick one of eleven operations at the top of the dialog. The fields below
it change to match. The four Duration operations work with the
[Time and Duration types](times-and-durations.md).

| Operation                               | Inputs                                    | Produces                                                                          |
|-----------------------------------------|-------------------------------------------|-----------------------------------------------------------------------------------|
| **Difference between two dates**        | Two date/datetime columns + a result unit | The gap between them, as a number in the chosen unit.                             |
| **Working days between two dates**      | A start and an end date column            | The working days from start to end, without days off and holidays.                |
| **Add / subtract time**                 | One date column + an amount + a unit      | The date shifted forward (or back) by that amount.                                |
| **Convert duration units**              | One numeric column + From / To units      | The same duration expressed in a different unit.                                  |
| **Extract a component**                 | One date column + a component             | A single field (year, month, weekday, …) pulled out of the value.                 |
| **Unix timestamp / date**               | One column + a direction + an epoch unit  | A Unix epoch number turned into a date/time, or a date turned into a Unix number. |
| **Convert timezone**                    | One date column + From / To zones         | The same instant expressed as wall-clock time in another zone.                    |
| **Duration between two times or dates** | A start and an end column                 | A Duration column holding end minus start.                                        |
| **Add a duration**                      | A time or date column + a duration        | The value moved by a duration from a column or by a fixed amount.                 |
| **Duration to a number**                | A Duration column + a unit                | The duration as a number of hours, minutes or another unit.                       |
| **Number to a duration**                | A number column + a unit                  | The number read in that unit, as a Duration.                                      |

### Difference between two dates

Choose a **First date** and a **Second date** column and a **Result
unit**. The result is `second - first`, so a later second date gives a
positive number. Units run from milliseconds up to years:
`Milliseconds`, `Seconds`, `Minutes`, `Hours`, `Days`, `Months`,
`Years`.

#### Against today, for example an age

Both date lists start with **Today**. Pick it instead of a column to
calculate against the current date: **First date** = the date of birth,
**Second date** = **Today**, **Result unit** = `Years` gives the age.
Years and months count whole calendar months, so the age comes out as
for example `34.5` (34 years and 6 months); round it down for the age in
whole years. **Today** is also offered for **Working days between two
dates** and **Duration between two times or dates**, for example the
working days left until a deadline.

The result is fixed when the column is added. It does not move on
tomorrow; run the calculation again for a fresh value.

### Working days between two dates

Choose a **Start** and an **End** column. The new column counts the days
from start to end that are worked:

- **Days off**: the days of the week nobody works, Saturday and Sunday
  to begin with. Tick the ones that apply, for example Friday and
  Saturday.
- **Holidays**: dates that are not counted either, one per line or
  separated by commas, in any date layout Octa reads (`2026-12-25`,
  `25.12.2026`, ...). Paste them from a column of holidays. A holiday
  that falls on a day off is not taken off twice.
- **Count the first and the last day**: on to begin with, so Monday to
  Friday is 5, as `NETWORKDAYS` counts in a spreadsheet. Off, the start
  day is not counted, so Monday to Friday is 4, like a difference in
  days.

An end before its start gives a negative number. The time of day is
ignored. A row without a date in either column stays empty and is
counted in the note above the table. An entry in **Holidays** that is
not a date is named under the field, and the column cannot be added
until it is fixed. The days off, the switch and the holidays stay for
the next working-days calculation until Octa closes.

An example, with Saturday and Sunday as days off and `2026-12-25` as the one
holiday:

| start      | end        | Working days |
|------------|------------|--------------|
| 2026-12-21 | 2026-12-24 | 4            |
| 2026-12-21 | 2026-12-31 | 8            |
| 2026-12-24 | 2026-12-21 | -4           |

Monday 21 to Thursday 31 December holds nine weekdays, and Christmas Day takes
one away. With **Count the first and the last day** off, the three rows give
3, 7 and -3.

### Add / subtract time

Choose a **Date column**, type an **Amount** (a whole number, e.g. `7`
or `-3`), and pick a **Unit**. Negative amounts subtract. Adding months
or years clamps the day to the length of the target month, so
`31 Jan + 1 month` lands on the last day of February rather than
overflowing into March.

### Convert duration units

Choose a **Number column** holding a duration, then a **From** and a
**To** unit. Octa rescales each value, e.g. minutes → hours divides by
60.

### Extract a component

Choose a **Date column** and a **Component** to pull out:

`Year`, `Month`, `Day`, `Hour`, `Minute`, `Second`, or
`Weekday (1=Mon..7=Sun)`.

### Duration between two times or dates

Choose a **Start** and an **End** column. Two clock times, two dates or
two datetimes give a Duration: end minus start. Two dates give whole days.
With **End before start means next day** ticked, an end time before the
start time counts as the next morning, so a night shift from `22:00` to
`06:00` lasts 8 hours instead of minus 16. The box applies to clock times
only.

**Difference between two dates** also reads two Time columns, and gives
the gap as a number in the chosen unit.

### Add a duration

Choose a **Start** column holding times, dates or datetimes, then either a
**Duration column** or tick **Fixed amount** and type one (`01:30:00`,
`26:00:00`, `PT90M`; a minus sign subtracts). A time wraps around at
midnight, so `22:00` plus 3 hours is `01:00`. A date or datetime moves by
the exact amount and comes out as a datetime; whole days onto a date
stay a date with **Add / subtract time**.

### Duration to a number, number to a duration

**Duration to a number** turns a Duration column into a number in the
unit you pick (`30:15:00` in hours is `30.25`). **Number to a duration**
reads a number column in the unit you pick (`90` minutes is `01:30:00`).
Months and years have no fixed length, so these two offer milliseconds
to days.

**Extract a component** pulls the hour, minute or second out of a Time
column too.

### Unix timestamp / date

Convert between a Unix epoch timestamp (a count since 1970-01-01
00:00:00 UTC) and a readable date/time, in either direction:

- **Direction** picks which way to convert. *Number to date/time* reads
  a numeric column of epoch values and produces a datetime; *Date/time
  to number* reads a date column and produces the epoch number.
- **Epoch unit** is the precision of the number: `Seconds`,
  `Milliseconds`, `Microseconds`, or `Nanoseconds`. Pick the one that
  matches your data, e.g. JavaScript `Date.now()` is milliseconds and
  most database/Unix tooling uses seconds.

The epoch is interpreted in **UTC**, with no timezone offset applied.
Nanosecond timestamps keep full precision (they are handled as 128-bit
integers, not floats), so a `seconds → date → nanoseconds` style round
trip is exact.

### Convert timezone

Read each datetime as wall-clock time in one zone and write it as
wall-clock time in another. Pick the **From zone** and **To zone** from
the full IANA list; the **Filter zones** box narrows both lists at once,
so typing `Berlin` or `America/` gets you there without scrolling 597
entries.

You have to state the source zone, because Octa cannot detect it.
Datetimes are stored without a timezone, so a value reading
`2024-01-15 12:00:00` carries no evidence of which zone it belongs to.

**Times that do not exist, or exist twice, are left empty and counted.**
Every zone that observes daylight saving has two such moments each year:
the clocks jump forward and an hour never happens, then they jump back
and an hour happens twice. `2024-03-31 02:30` never occurs in
`Europe/Berlin`, and `2024-10-27 02:30` occurs twice. There is no single
correct answer for either, so Octa refuses to invent one and tells you
how many cells it left empty:

> *N cells were ambiguous and left empty*

## How values are read

Date and datetime columns are read directly. Plain-text columns are run
through the same [date inference](../reference/date-inference.md) parser
the table uses, so ISO and common European/US layouts are understood
even when the column is still typed as text. Duration conversion reads
the source column as a number.

## When a cell can't be computed

Cells that aren't valid dates (for the date operations) or aren't valid
numbers (for duration conversion) are skipped, and a banner appears
above the table:

> *Date/Time calculation skipped N of M row(s) (cells that aren't valid
> dates/numbers).*

The new column still materialises; skipped rows are left empty. The
amount field for **Add / subtract time** must be a whole number,
entering a fraction shows an inline error instead of running.

## See also

- [Formulas](formulas.md) for arithmetic on numeric columns.
- [SQL panel](sql.md) for DuckDB date functions, joins, and aggregates.
- [Date inference](../reference/date-inference.md) explains how text
  columns are recognised as dates.
- [Editing](editing.md) covers the other column-creating dialogs.
