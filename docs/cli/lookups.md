# `--lookups`

Hidden lookup tables: columns that always have the same value for the same
key, like a customer's name and city repeated next to the customer ID on every
order row. A flat export like that really holds two tables, and a key whose
rows disagree (one customer spelt two ways) is usually a typo.

```sh
octa --lookups orders_flat.csv
octa --lookups orders_flat.csv --lookups-min 0.8
```

## Flags

| Flag            | Required? | Description                                                            |
|-----------------|-----------|------------------------------------------------------------------------|
| `--lookups`     | yes       | The file.                                                              |
| `--lookups-min` | no        | Share of rows (0 to 1) that must agree with their key. Default `0.95`. |

## Output

One row per key and following column. For
`octa --lookups samples/features/orders_flat.csv --lookups-min 0.8`:

```text
key       follows   consistency_percent  conflicting_keys  breaking_rows
customer  city      100.0                0                 0
customer  name      87.5                 1                 1
name      customer  100.0                0                 0
name      city      100.0                0                 0
city      customer  100.0                0                 0
city      name      87.5                 1                 1
```

A key can work both ways round: `name` decides `customer` too, since every
spelling of a name belongs to one customer. At the default `0.95` the two
87.5% rows drop out.

`consistency_percent` counts the rows of keys that occur more than once and
the share of them carrying their key's most common value. A column with more
than half as many distinct values as rows is never a key. The number of keys
found goes to stderr.

See [Find lookup tables](../usage/lookup-tables.md).
