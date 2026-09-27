# Test Data

You want to show a colleague, a supplier or a bug report how your table
looks, but the data is customers, salaries or patients. **Data -> Generate
test data...** makes new rows shaped like the real ones: the same columns,
the same kind of values, nothing real.

## What each column becomes

Octa looks at every column once and picks how to make it:

| Column looks like                          | Generated as                                                                              |
|--------------------------------------------|-------------------------------------------------------------------------------------------|
| An ID: whole numbers, all different        | **Running number** 1, 2, 3, ...                                                           |
| Numbers                                    | **Number, real spread**: drawn from the real percentiles, so a skewed column stays skewed |
| Dates, date and time                       | The same, within the real period                                                          |
| Yes / no                                   | The real share of yes                                                                     |
| Names, emails, phones, IBANs, card numbers | **Fake** values of that kind (the same detection as [Detect PII](detect-pii.md))          |
| A city or company column                   | Fake cities or companies                                                                  |
| A few values that repeat (status, country) | **Categories, real values** at their real frequencies                                     |
| Codes like `ORD-00123`                     | **Same shape**: the prefix kept, digits and letters random (`ORD-48213`)                  |
| Other text                                 | **Neutral words** of a similar length                                                     |

Every column also gets **empty cells at the real rate**: a column that is
25% empty in the real table is about 25% empty in the test data.

A fake IBAN or card number is not just the right length: it passes its
check digit, so the [ID checks](data-validation.md#checking-ids-iban-card-numbers-barcodes-vat-email)
treat it as valid. Fake phone numbers come from the UK's range reserved for
films and television, fake IP addresses from the ranges reserved for
documentation, and fake card numbers from a test range no bank issues.

## The one place real values appear

A category column (a status, a country, a product line) keeps its **real
values**, because "paid / open / refunded" is usually what makes test data
useful. The dialog marks these columns **real values** in orange. If even
those must not appear, pick **Categories, renamed** in the column's
dropdown: the same frequencies, with `value_1`, `value_2` and so on in
place of the real words.

Everything else is made up.

## Several tables that link

Tick more than one tab. When one table points at another (an
`orders.customer_id` pointing at `customers.id`), Octa finds the link with
the [Join Key Finder](join-key-finder.md) and draws the child column only
from the parent's **generated** IDs. So the test orders still belong to
test customers, and a [join](join-tables.md) of the two test tables works
exactly like one of the real tables. The column shows **Values of
customers.id** in the dropdown; you can pick that for any column yourself,
or pick something else to break the link.

Two columns that are both unique (for example two running numbers that
both start at 1) are never linked by guesswork, because there is no
telling which one is the parent.

## Using it

1. Open the table (or tables) and **Data -> Generate test data...**
2. Tick the tables to imitate. The active tab starts ticked.
3. Check the grid: every column with the way it will be made. Change any
   of them in its dropdown.
4. **Rows per table**: leave it empty for as many rows as the real table
   has, or type a number (ten rows for a bug report, a million for a load
   test).
5. **Seed**: the same seed with the same settings gives exactly the same
   rows, so a test set can be made again later. It starts at a new value
   each time the dialog opens.
6. **Generate** opens one new tab per table, named **Test data: ...**,
   ready to save in any format.

## From the command line

```bash
octa --test-data customers.csv orders.csv --test-data-rows 1000 --seed 42 \
     --test-data-out testdata/
```

Several inputs are generated together, so their links hold, and land in
the folder as `customers_test.csv` and `orders_test.csv`. The plan (which
generator made each column) goes to stderr, with a note for every column
that kept its real values. See [`--test-data`](../cli/test-data.md).

## For the assistant

The `generate_test_data` tool does the same and returns the tables. See
[`generate_test_data`](../mcp/tools/generate_test_data.md).

## Limits

- **Every column is made on its own.** Relationships between columns in
  one table are not kept: big orders do not go to big customers, and an
  end date can come out before its start date.
- Links are found between tables, not within one (a `manager_id`
  pointing at `id` in the same table is not linked automatically, but
  you can pick **Values of ...** for it).
- Fake names and cities come from small built-in lists, so they repeat
  in a large table.
- `samples/features/customers.csv` and `orders.csv` in the repository
  are a linked pair to try it on.
