# How the Assistant Understands Your Data

There is a difference between querying data and understanding it. A
query can be flawless SQL and still answer the wrong question, because
it read `date` as the order date when it was the shipping date, or
joined on the column that happened to be called `id`.

Octa's answer is that the assistant does not interpret your data by
reading its column names. It measures. This page describes the layer
that sits between your question and the SQL that answers it, and it
works through one small example in full.

## What the assistant knows when the conversation starts

Almost nothing. The system prompt tells the model which tabs are open,
how many rows they hold, and how many columns:

```text
Open tabs right now:
- #1 "orders.parquet" (active): 4812993 rows, 11 columns
- #2 "customers.csv": 91204 rows, 7 columns
```

That is deliberately all of it. No column names, no types, no sample
rows. The prompt then instructs the model to call `schema` or
`describe_file` to orient itself before reading anything.

The effect is that it cannot form an opinion about a column before it
has looked at one. A guess such as "the column is called `amount`, so it
is the order total" has to survive a `profile` call showing that the
column is 94% null and ranges from -1 to 3.

## The tools it can reach for

Every tool below is a local, deterministic engine. None of them consults
a language model, and the same table always produces the same answer.

| Tool                      | The question it answers                                                   |
|---------------------------|---------------------------------------------------------------------------|
| `schema`, `describe_file` | What columns exist, of what type, and how the file is physically laid out |
| `profile`                 | Per column: nulls, distinct count, min, max, quartiles                    |
| `value_frequency`         | What a column actually contains, and how often                            |
| `unique_columns`          | Which column, or combination of columns, identifies a row                 |
| `suggest_join_keys`       | Which columns across two tables would genuinely join                      |
| `diagnose_join`           | Why a join that should have worked returned too few rows                  |
| `detect_pii`              | Which columns hold personal data, by header and by value pattern          |
| `detect_outliers`         | Which individual values sit outside the column's normal range             |
| `correlation`             | Which numeric columns move together                                       |
| `schema_drift`            | Which files in a folder disagree about their columns                      |

The assistant calls these the way you would use the menu entries, and
each has its own page in this documentation. Nothing here is a special
assistant-only capability.

## A worked example

Two small tables, deliberately full of repeated values. `orders`:

| order_id | customer_id | ship_region | amount |
|----------|-------------|-------------|--------|
| 1        | 3           | north       | 42.50  |
| 2        | 3           | north       | 17.50  |
| 3        | 1           | south       | 8.25   |
| 4        | 4           | north       | 61.00  |
| 5        | 4           | south       | 12.75  |
| 6        | 4           | north       | 5.10   |
| 7        | 2           | south       | 30.00  |
| 8        | 2           | south       | 9.99   |

And `customers`:

| id | region | name    |
|----|--------|---------|
| 1  | south  | Adams   |
| 2  | south  | Boateng |
| 3  | north  | Cruz    |
| 4  | north  | Dlamini |

You ask which customers spent the most. Three columns on the left could
plausibly be the link, two on the right, and one of the candidates is
about to be wrong in a way that still returns rows.

### Step one: the model learns the column names

It calls `schema` and, for the first time in the conversation, sees the
seven column names and their types. It still has not seen a value.

### Step two: it measures, it does not guess

It calls `suggest_join_keys`, which scores **every** column of the left
against **every** column of the right. Names take no part in the score.
For each pair, over a sample:

```text
overlap  = shared distinct values / smaller distinct set
distinct = distinct values / non-empty values sampled   (per side)
score    = overlap * the better distinct of the two sides
```

Pairs sharing no values at all never appear, which removes most of them
here: `amount` has nothing in common with `region`, `ship_region`
nothing with `id`. Three candidates survive:

| Pair                                      | Overlap | Distinct left | Distinct right | Score |
|-------------------------------------------|---------|---------------|----------------|-------|
| `orders.order_id` → `customers.id`        | 100%    | 100%, 8 of 8  | 100%, 4 of 4   | 1.00  |
| `orders.customer_id` → `customers.id`     | 100%    | 50%, 4 of 8   | 100%, 4 of 4   | 1.00  |
| `orders.ship_region` → `customers.region` | 100%    | 25%, 2 of 8   | 50%, 2 of 4    | 0.50  |

### What the duplicates do to the score

`customer_id` repeats, because customer 4 placed three orders. Only four
distinct values across eight rows, so its distinctness is 50%. That is
not a defect and it is not punished: the score takes the **better** of
the two sides, so a many-to-one relationship scores exactly as well as a
one-to-one. Only the parent side has to be unique, and `customers.id`
is.

`ship_region` is the instructive one. Its two values, `north` and
`south`, are fully contained in the region column, so its overlap is a
perfect 100%. On names alone, "region matches region" reads convincing.
But both sides repeat heavily, neither is anywhere near unique, and the
score halves to 0.50. A column that repeats on **both** sides is a
category, not a key, and the second factor exists precisely to say so.

### Where values alone are not enough

The top two tie at 1.00, and both are honest. The customer IDs 1 to 4
really are all present among the order IDs 1 to 8, both columns really
are perfectly distinct on the side that matters, and comparing values
cannot separate them. That happens whenever two tables number their rows
from 1 upwards, which plain counters do and UUIDs, prefixed codes and
natural keys do not.

So each candidate also reports its **orphans**: how many distinct values
on one side find no partner on the other, counted over the same sample so
the figures can never contradict each other, and counted **both ways
round** since only one of the two directions settles a tie like this one.

| Pair                                  | Orphans                   |
|---------------------------------------|---------------------------|
| `orders.order_id` → `customers.id`    | 4 of 8: values 5, 6, 7, 8 |
| `orders.customer_id` → `customers.id` | 0 of 4                    |

That settles it. Joining on `order_id = id` returns four rows, each one
a coincidence of numbering, and four rows is exactly the kind of result
that looks like an answer. Joining on `customer_id = id` returns all
eight orders with their customer attached.

### What the model saw, and what it did not

| It saw                                             | It never saw                                 |
|----------------------------------------------------|----------------------------------------------|
| The tab names and their row and column counts      | The eight order rows                         |
| The seven column names and types, from `schema`    | The four customer rows                       |
| The three candidates above: about fifteen numbers  | Any name, amount, region or ID value         |
| The rows of the final answer, once it ran the join | Everything the comparison read and discarded |

Both tables were read in full on your machine. What crossed to your
model provider was the conclusion. On eight rows the saving is academic;
on the 4.8 million rows and 91,204 rows of the tab listing further up,
ranking every candidate pairing still costs a short block of JSON
holding roughly twenty numbers.

See [Join Key Finder](join-key-finder.md) for the same ranking as a
dialog you can drive yourself.

## When nothing is suggested at all

Change one thing in the example: `customers.id` was exported through a
spreadsheet and now reads `001`, `002`, `003`, `004`, while
`orders.customer_id` still reads `1` to `4`.

Those columns now share no values whatsoever, so the pair scores nothing
and is not returned. The finder going quiet is the signal, and
`diagnose_join` is the tool for it. Given the pair you believe in, it
reports:

```text
left rows 8, right rows 4
distinct keys      4 left, 4 right
matched            0 left, 0 right
unmatched (left)   3, 1, 4, 2
unmatched (right)  001, 002, 003, 004
suggested fix      strip leading zeros -> 4 of 4 would match
```

Four properties of that report are deliberate:

- Counts are over **distinct keys**, not rows. A join failing on three
  IDs is one problem, however many rows carry them.
- It shows up to five **real unmatched values** per side, from the key
  column only, because you cannot see a trailing space or a lost leading
  zero in a count.
- It names the single normalisation that would help: trim whitespace,
  ignore case, collapse repeated spaces, strip punctuation, or strip
  leading zeros. A fix is listed **only when it strictly beats** the
  current number of matching keys, so an empty list is a real answer
  meaning the values genuinely do not correspond, not a shrug.
- The baseline compares values byte for byte, untrimmed. Trimming first
  would silently repair the very problem the trim suggestion exists to
  report.

See [Join Diagnostics](join-diagnostics.md).

## Seeing a whole folder at once

Two tables is a question you can hold in your head. Thirty exports in a
folder is not. The **relationship map** runs the same ranking across
every pair of tables, over the tabs you have open or over a folder, and
draws the result: one box per table listing its columns, a line between
each pair of columns that relate, boxes you can drag into an
arrangement that makes sense to you.

Every line is labelled in plain words rather than a score, for example
"98 of 100 values in orders.customer_id exist in customers.id; 12 have
no match". Clicking a line prefills the ordinary
[Join](join-tables.md) dialog with that pair.

Reading a folder means reading values, so a folder scan runs on a
background thread with a cancel button and the same row cap as
everything else here.

## What reaches your model provider

The measurement happens inside Octa on your machine. What crosses is the
result, and for most of these tools that means no cell values at all.

| Tool                                                 | Cell values sent to the provider                                                         |
|------------------------------------------------------|------------------------------------------------------------------------------------------|
| `suggest_join_keys`                                  | **None.** Table labels, column names, and a handful of numbers per candidate             |
| `profile`, `count_rows`, `unique_columns`            | **None.** Aggregates only                                                                |
| `diagnose_join`                                      | Up to five unmatched key values per side, so ten strings at most, from the key column    |
| `value_frequency`                                    | The values of one column, with their counts                                              |
| `read_table`, `run_sql`, `join_tables`, `fuzzy_join` | Yes, real rows. This is the data itself, capped by the chat row limit and 4 KiB per cell |

If your active model profile points at a local model through Ollama,
nothing leaves your machine at any point. The full picture is in the
[privacy policy](../privacy.md).

## The Ask boxes are a smaller thing

The plain-language **Ask** boxes in the search bar and the SQL panel are
not the assistant. Each sends **one** request with no tools and no agent
loop, so a search box can never turn into an autonomous session.

Ask SQL is given the table name, the SQL dialect, the row count and the
list of columns with their types, and must reply with a single `SELECT`.
The answer is spliced into the editor at your cursor and **is never
run**. You read it and press Run yourself. Anything that is not one
`SELECT` statement is rejected outright, and a rejected reply applies
nothing rather than half a query.

That split is the point. The Ask boxes are fast and shallow. The
assistant is the one that can go and measure first.

## What this does not do

- **The model chooses whether to measure.** The prompt tells it to
  orient itself first and the tools are there, but nothing forces a
  call. If you want certainty, run the Join key finder or the
  relationship map yourself from the **Analyse** menu before asking.
- **Sampled**, 10,000 rows per table by default. Strong evidence, not a
  certified foreign key. Raise the sample when it matters.
- **Single columns only.** Composite keys are not suggested; use
  [`--unique-columns`](../cli/unique-columns.md) for that question.
- **Values are compared as trimmed text**, so `1` and `1.0` are
  different keys.
- **The first table only** of a multi-table source such as a workbook.
- **Orphan counts separate candidates, they do not choose.** Two tables
  that genuinely both relate to a third will both show few orphans, as
  they should. The map shows you the shape; the meaning is still yours.
