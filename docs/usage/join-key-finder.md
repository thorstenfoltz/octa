# Join Key Finder

<!-- SCREENSHOT: join-key-finder.png: The Join key finder with three tables ticked in the Tables row, the sample-size field showing 10000, and a ranked result list below where the top entry reads `orders.cust_id -> customers.id  98% overlap, 100% distinct` with a Use in Join button. -->

**Analyse → Join key finder...** ranks the column pairs that would
actually join the tables you have open.

Joining two unfamiliar tables normally goes: guess the key columns, run
the join, get zero matched rows, guess again. This removes the guessing
by looking at the values rather than the names.

## How the score is calculated

Everything below is plain set arithmetic on the values themselves. It is
worth understanding, because the number it produces is the whole answer.

### Step 1: turn each column into a set

For every column of every ticked table, Octa reads the first
**10,000 rows** (the **Sample rows per table** box) and builds:

- **the distinct values** of that column, and
- **how many non-empty values it saw**, which becomes a denominator.

Each cell is turned into text and trimmed, and empty cells are **skipped
entirely** - they count towards neither number. So `1` in an integer
column and `"1"` in a text column are the same value, `1` and `1.0` are
not, and a column that is half nulls is judged on the half that is there.

### Step 2: score every pair of columns

For each column of table A against each column of table B:

```text
shared       = how many values appear in BOTH distinct sets
overlap      = shared / the smaller of the two distinct counts
distinctness = distinct values / non-empty values sampled   (each side)

score        = overlap x the LARGER of the two distinctness values

orphans      = distinct values on this side - shared    (counted BOTH ways)
```

A pair sharing no value at all is not a candidate. A pair scoring below
**0.2** is dropped as noise, and the [Relationship map](relationship-map.md)
raises that floor to **0.50** by default because a picture of every weak
pair is not a picture.

Since `overlap` and both distinctness values are between 0 and 1, so is
the score.

### Why the larger distinctness and not the smaller

This is the part that looks wrong and is not. A real foreign key is
**unique on one side only**: `customers.id` has a different value in
every row, while `orders.cust_id` repeats it once per order. Its
distinctness is deliberately low, and taking the smaller of the two would
punish exactly the shape we are looking for. Taking the larger asks the
right question, "is at least one of these two columns something that
identifies a row?", which is true of every key pairing and false of a
`status` column on both sides.

### A worked example

Two files, small enough to check by hand:

`customers.csv`

```text
id,name,status
c1,Alice,open
c2,Bob,shut
c3,Cara,open
c9,Dan,open
```

`orders.csv`

```text
order_id,cust_id,status
1,c1,open
2,c1,shut
3,c2,open
4,c3,open
5,c3,open
```

Take **customers.id against orders.cust_id**:

| Quantity              | Working                         | Value    |
|-----------------------|---------------------------------|----------|
| distinct on the left  | `c1 c2 c3 c9`                   | 4        |
| distinct on the right | `c1 c2 c3`                      | 3        |
| shared                | `c1 c2 c3`                      | 3        |
| overlap               | 3 / min(4, 3)                   | **1.00** |
| distinctness left     | 4 distinct / 4 non-empty values | **1.00** |
| distinctness right    | 3 distinct / 5 non-empty values | 0.60     |
| score                 | 1.00 x max(1.00, 0.60)          | **1.00** |
| orphans               | 4 - 3                           | **1**    |

The orphan is `c9`, the customer who has never ordered. That is a fact
about your data, not a fault in the pairing.

Now the trap, **customers.status against orders.status**. Both columns
hold exactly `open` and `shut`, so they overlap **perfectly** and leave
**zero orphans**:

| Quantity           | Working                         | Value    |
|--------------------|---------------------------------|----------|
| overlap            | 2 / min(2, 2)                   | **1.00** |
| distinctness left  | 2 distinct / 4 non-empty values | 0.50     |
| distinctness right | 2 distinct / 5 non-empty values | 0.40     |
| score              | 1.00 x max(0.50, 0.40)          | **0.50** |

Perfect overlap, no orphans, and still only half the score of the real
key. Distinctness is the only thing separating them, which is precisely
its job.

Both rows are exactly what Octa prints for those two files:

```console
$ octa --relationships ./example
left_table     left_column  right_table  right_column  score  overlap  left_orphans  left_values  right_orphans  right_values
customers.csv  id           orders.csv   cust_id       1      1        1             4            0              3
customers.csv  status       orders.csv   status        0.5    1        0             2            0              2
```

And the trap gets weaker the more real the data is. Grow those files to
2,000 customers and 10,000 orders with the same three statuses and the
status pair scores `1.00 x 3/2000 = 0.0015`, far under the 0.2 floor, so
it stops being reported at all - while the key pairing still scores 1.00,
now with 212 orphans on the customers side (people who never ordered) and
none on the orders side (every order belongs to a real customer).

### Which side is "left"

`overlap` and `score` are **symmetric**: swapping the two columns gives
the same number. `orphans` is **not**, so it needs a direction - and since
nothing here knows which of your two tables is the child, **both
directions are always reported**. "Left" is simply the table that came
first (the earlier tab, or the earlier filename in a folder), not a claim
about which one is the parent.

## Orphans break the ties

Two candidates can score identically and only one of them be real. It
happens whenever both tables number their rows from 1, which is most
tables with an auto-increment key. Here are two such files:

`customers.csv` (4 rows)

```text
id,name
1,name1
2,name2
3,name3
4,name4
```

`orders.csv` (1,000 rows, every order placed by one of those four
customers)

```text
id,customer_id,amount
1,3,64
2,1,17
3,4,92
...
1000,2,45
```

Two pairings tie at the top, and only one of them means anything:

- `customers.id` against **`orders.customer_id`** is the real foreign
  key.
- `customers.id` against **`orders.id`** is a coincidence: order numbers
  1 to 4 exist simply because there are more than four orders.

Both score a perfect 1.00, because both have full overlap and a side that
is completely distinct. So each candidate also reports its **orphans**,
the distinct values on one side with no partner on the other - **counted
both ways round**, because only one of the two directions can settle this
and nothing in the arithmetic knows which of your tables is the child:

```console
$ octa --relationships ./tiebreak
left_table     left_column  right_table  right_column  score  overlap  left_orphans  left_values  right_orphans  right_values
customers.csv  id           orders.csv   id            1      1        0             4            996           1000
customers.csv  id           orders.csv   customer_id   1      1        0             4            0             4
```

Read from the customers side, the two lines are identical: all four
customer ids appear in both of the columns they are compared against, so
`left_orphans` is 0 either way. Read from the orders side they are
decisive: **996 of the 1,000 order numbers point at no customer at all**,
while every `customer_id` finds one.

That is the whole tie broken, and it no longer depends on which table you
happened to open first. Whichever way round the two tables come, the pair
of numbers is the same pair; only their labels swap.

A relationship map built from a live database does not need the second
number to decide anything: a declared foreign key already knows which end
is the child, and **Measure** scores it in that direction, so its
`left_orphans` is always "child rows pointing at a parent that is not
there".

The [Relationship map](relationship-map.md) shows both counts on every
line it draws, and `suggest_join_keys` returns them as `left_orphans` out
of `left_distinct_values` and `right_orphans` out of
`right_distinct_values`.

## No language model is involved

This is set arithmetic over the sampled values, computed on your own
machine. Nothing is sent anywhere, no model is consulted, and the same
tables always produce the same ranking. The only features in Octa that
talk to a language model are the Chat assistant and the two
plain-language **Ask** boxes, all listed in the
[privacy policy](../privacy.md).

Results read like this:

```text
orders.cust_id -> customers.id    98% overlap, 100% distinct   [Use in Join]
```

## Using it

1. Open the tables you want to join.
2. **Analyse → Join key finder...** The active tab and one other are
   ticked for you; tick more to compare three or more tables, which
   gives you every pairing between them.
3. Adjust **Sample rows per table** if you want (default 10,000; more is
   slower and more certain), then press **Scan**.
4. **Use in Join** opens the ordinary [Join](join-tables.md) dialog with
   that table pair and condition already filled in, where you pick the
   join type and run it.

The finder never joins anything itself. It only tells you which columns
are worth joining on, so there is one join implementation and one place
where join options live.

## Over MCP

`suggest_join_keys` gives an agent the same ranking before it calls
`join_tables`:

```json
{ "paths": ["orders.csv", "customers.csv"] }
```

It returns `candidates` best first, each carrying `left_table`,
`left_column`, `right_table`, `right_column`, `overlap`, `left_distinct`,
`right_distinct`, `score`, and both orphan counts - `left_orphans` out of
`left_distinct_values` and `right_orphans` out of
`right_distinct_values`, all as defined above. It is read-only and stays
available under `--mcp-read-only`.

## Ceilings

- **Sampled**, so 98% overlap is strong evidence, not proof. Raise the
  sample if it matters.
- **Orphans are counted in both directions**, but nothing labels which
  table is the child; you read that off the two numbers. See
  [Orphans break the ties](#orphans-break-the-ties).
- **Single columns only.** Composite keys (two columns together
  identifying a row) are not suggested; use
  [`--unique-columns`](../cli/unique-columns.md) for that side of the
  question.
- Values are compared as trimmed text, so `1` matches `1` across a
  numeric and a text column. That is usually what you want when the two
  sides came from different systems.

<!-- TODO screenshot: the Join key finder with three tables ticked and a
     ranked result list. Listed in docs/assets/screenshots/INDEX.md. -->
