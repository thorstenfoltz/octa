# Find lookup tables

A flat export often glues two tables together: every order row repeats the
customer's name and city next to the customer ID. **Analyse -> Find lookup
tables...** finds such columns, shows the rows that break the pattern, and can
split the lookup back out into its own table.

The scan starts as soon as the dialog opens. It compares every pair of columns
of the active tab (unsaved edits included) and runs in the background, so you
can keep working or press **Cancel**.

## Example

`samples/features/orders_flat.csv` has eight orders for three customers:

| order | customer | name    | city    | amount |
|-------|----------|---------|---------|--------|
| 1     | c1       | Acme    | Berlin  | 10     |
| 2     | c1       | Acme    | Berlin  | 20     |
| 3     | c2       | Mueller | Munich  | 30     |
| 4     | c2       | Mueller | Munich  | 40     |
| 5     | c2       | Muller  | Munich  | 50     |
| 6     | c3       | Zeta    | Hamburg | 60     |
| 7     | c3       | Zeta    | Hamburg | 70     |
| 8     | c1       | Acme    | Berlin  | 80     |

With the threshold at 80%, one of the findings is `customer` deciding two columns:
`city` at 100% and `name` at 88% with one row that breaks it (order 5, where
Mueller is spelt Muller).

## The threshold

**At least** sets how consistently a column must follow the key to be listed.
Consistency counts the rows of keys that occur more than once and asks how many
of them carry their key's most common value. 100% lists only perfect lookups;
the default of 95% also finds lookups with a few mistakes, which are usually
the interesting ones. Change the number and press **Scan again**.

A key column must repeat: a column with more than half as many distinct values
as rows (an order number, say) is never a key, since a unique column trivially
decides everything else.

## The buttons

Each finding has a tick box per column. Both buttons use only the ticked
columns.

- **Show breaking rows** opens a new tab with every row of every key whose
  ticked columns disagree, grouped by key. Order 5 and the two other rows of
  customer c2 in the example.
- **Split out** opens two new tabs: the lookup table (one row per key, with the
  ticked columns) and the original table without the ticked columns. Where a
  key had conflicting values, the lookup takes the most common one and a banner
  says how many keys that affected.

The tab you scanned is never changed. Save the new tabs if you want to keep
them.

## Limits

- Values compare by their displayed text, as in the column filter.
- Keys are single columns. A lookup that needs two columns together as its key
  is not found.
- On a partially loaded file, the scan sees only the loaded rows; the dialog
  says so. A database tab that holds only part of its table is scanned on
  the database over every row instead; see
  [Analyses on live databases](analyses-on-live-databases.md#keys-and-relationships).

## See also

- [Data quality report](data-quality-report.md) for per-column checks.
- [Referential integrity](referential-integrity.md) to check a split-out key
  against another table.
