# Data Quality Report

**Analyse > Data quality report...** opens a new tab that scores each column of
the active table, so you can see at a glance where the data needs cleaning.

## What it shows

One row per source column, with these metric columns (hover any header for a
full explanation):

| Column                  | Meaning                                                                         |
|-------------------------|---------------------------------------------------------------------------------|
| `null_percentage`       | Percentage of missing (null) values.                                            |
| `distinct_ratio`        | Distinct values divided by non-null values (1.0 = all unique).                  |
| `outlier_count`         | Number of numeric outliers (IQR method, as in Detect outliers).                 |
| `pii_flag` / `pii_kind` | Whether the column looks like personal data, and what kind (reuses Detect PII). |
| `type_consistency`      | Share of values that match the column's declared type.                          |
| `score`                 | Overall 0-100 quality score for the column.                                     |

The **score** is a mark out of 100 for one column, worked out from three of the
other columns: 40% for how much of it is filled in (`null_percentage`), 40% for
how much of it matches the declared type (`type_consistency`), and 20% for how
varied the values are (`distinct_ratio`). Numeric outliers take up to 10 points
off. It says nothing about whether the values are *correct*, only about how well
formed they are, so a column of neatly typed but wrong dates still scores well.

The overall table score is the mean of the column scores. It appears in the tab
title (`Quality 81/100 - sales.parquet`) so it stays on screen, and in the
status bar when the report opens. Hovering the tab repeats the explanation, and
hovering the `score` column header explains a single column's mark.

The report is an ordinary table tab: sort it, filter it, or save it like any
other file. Re-run the report after cleaning to watch the score improve.

### Hover a verdict to see what it means

The two verdict columns hold labels rather than data, and a label short enough
for a cell cannot also say what it means. **Hovering one explains it**: what
the verdict is telling you, and what it is not.

That matters most for the values that are not verdicts at all. Every one of
them starts `not tested:` and names the gate it tripped, and the hover says why
that gate exists. `not tested: narrow range` is not a complaint about the
column; it is Octa declining to judge a percentage by a law percentages have no
reason to follow.

The column header still carries its own tooltip describing the column as a
whole, so the two answer different questions: what is measured here, and what
does this answer mean.

### Benford's law

`benford_verdict` asks whether a numeric column's **leading digits** look like
measured quantities. In numbers that arise from measuring or accumulating
things across several orders of magnitude, a leading 1 turns up about 30% of
the time and a leading 9 under 5%. Invented, transcribed or capped figures
usually do not do that, which is why auditors reach for this test.

The verdict is the mean absolute deviation from the expected shares, in
Nigrini's bands:

| Value           | Meaning                                           |
|-----------------|---------------------------------------------------|
| `conforms`      | Close to the expected distribution.               |
| `acceptable`    | Slightly off, within the range real data wanders. |
| `marginal`      | Far enough off to be worth a second look.         |
| `nonconforming` | The digits do not look like measured quantities.  |

**Most columns get no verdict at all, and that is the point.** A column that
reads `nonconforming` when it had no reason to follow the law in the first
place teaches you to ignore the column. So four gates come first, and a column
that trips one says which:

| Value                        | Why the test does not apply            |
|------------------------------|----------------------------------------|
| `not tested: not numbers`    | The column is not numbers.             |
| `not tested: too few values` | Fewer than 300 usable values.          |
| `not tested: narrow range`   | Values span less than a factor of ten. |
| `not tested: looks like ids` | Dense, distinct whole numbers.         |

Below 300 values the digit shares are noise rather than a distribution. A
column spanning less than a factor of ten is a **bounded range** - a
percentage, an age, a rating - and a bounded range has no reason to follow the
law. Dense, distinct whole numbers are an **assigned sequence** - a row number,
an invoice number, an id - and those are handed out, not measured.

The identifier gate tests **density, not uniqueness**. Fibonacci numbers are
whole and all different and famously *do* follow the law; they are also spread
across their range by a factor of thousands, so they stay in. A run of 1 to
1000, even with every twentieth number missing, does not.

A verdict is evidence to look further, never a finding on its own. A
nonconforming column can be perfectly innocent, and a conforming one can still
be wrong.

### Calendar coverage

`calendar_verdict` walks a time column's calendar. A daily series with four
days missing in March looks perfectly healthy in every other statistic on this
report - no nulls, no outliers, a sensible range - and the only way to see it
is to check the dates one by one.

Octa infers the column's **step** from the most common distance between
consecutive timestamps, then looks for holes in it:

| Value                     | Meaning                                        |
|---------------------------|------------------------------------------------|
| `complete`                | No holes.                                      |
| `weekdays only`           | Every hole is a skipped weekend.               |
| `complete (clock change)` | The only hole has the shape of a clock change. |
| `gaps`                    | Time that should have rows and does not.       |

**Two false alarms are ruled out rather than reported**, because either one
would make the check useless:

- **A weekday-only series is not broken.** Business data skips Saturdays and
  Sundays on purpose, and a report that flags 104 gaps a year for it is a
  report nobody reads twice. Both ends have to line up, so a Friday-to-Monday
  hole is a weekend and a Friday-to-Wednesday hole is still missing data.
- **A daylight-saving change is not missing data.** Octa's timestamps carry no
  timezone, so a spring-forward reads as a one-hour hole. There is nothing to
  check against, so the *shape* is used instead: one step missing, on a Sunday,
  in the small hours, in a series finer than a day. That is a heuristic, which
  is why it gets its own verdict rather than being silently swallowed - the
  same hole on a Wednesday afternoon is reported as missing.

Columns that are not a series say why:

| Value                         | Why the check does not apply               |
|-------------------------------|--------------------------------------------|
| `not tested: not dates`       | The column is not dates or timestamps.     |
| `not tested: too few points`  | Fewer than three distinct timestamps.      |
| `not tested: no regular step` | No step accounts for 60% of the intervals. |

### Calendar gaps

The holes themselves are too long for a cell, so they open in their own tab:
one row per gap, with the column, the timestamps either side of it, how many
steps are absent, and whether it was counted as `missing` or excused as
`daylight_saving`.

**Weekends are deliberately not listed.** A five-year weekday series has 260 of
them and every one is expected; `weekdays_only` on the main table is the whole
finding. What lands in this tab is what someone would have to go and look into.
Capped at 200 gaps: a column with more than that is telling you one thing, and
it is not the list.

## Extra tabs for findings that are not per column

Some problems do not fit a one-row-per-column table, so they open in their own
tab beside the report. Focus stays on the main tab, since the score is what you
asked for, and the status bar says how many extra tabs appeared. A finding with
nothing to report opens no tab at all, so a clean file still gives you exactly
one.

An extra tab arrives without your having asked for it by name, so it introduces
itself: hover the tab for a one-sentence answer to "what am I looking at?", and
hover any column header in it for what that column holds.

### Missing together

"This column is 12% null" is already on the main table, and on its own it does
not tell you much. Four columns each 8% null are a completely different problem
depending on whether they are empty in the **same** rows, which usually means
one upstream join that did not match, or in different ones, which means four
unrelated gaps.

This tab lists the sets of columns that go missing together:

| Column          | Meaning                                        |
|-----------------|------------------------------------------------|
| `columns`       | The columns that are empty in the same rows.   |
| `column_count`  | How many columns that is.                      |
| `rows`          | How many rows share exactly that set of holes. |
| `share_percent` | Those rows as a percentage of the table.       |

Biggest pattern first. Two rules keep it quiet:

- **A column that is null on its own is not a pattern.** That is the null
  percentage the main table already gives you, and repeating it here would
  bury the multi-column findings this tab exists for.
- **A handful of rows is not a pattern either.** A set has to cover at least
  1% of the table and at least 5 rows, so a small file does not report noise.

An empty text cell counts as missing alongside a real null: it is a hole in the
data whatever the file called it, and CSV readers differ on which one they
produce.
