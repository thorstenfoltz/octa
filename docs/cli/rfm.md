# `--rfm`

Customer segments (RFM): every customer marked 1 to 5 for how recently, how
often and how much they bought, and the segment name read off the marks. The
same engine as **Analyse > Customer segments (RFM)...**; see
[Customer segments (RFM)](../usage/rfm.md).

```sh
octa --rfm orders.csv --rfm-customer customer --rfm-date date --rfm-amount amount
octa --rfm lines.parquet --rfm-customer email --rfm-date ordered_at --rfm-amount total \
     --rfm-order order_no --rfm-as-of 2024-12-31 --rfm-names eleven
```

## Flags

| Flag             | Required? | Description                                                                                            |
|------------------|-----------|--------------------------------------------------------------------------------------------------------|
| `--rfm`          | yes       | The file.                                                                                              |
| `--rfm-customer` | yes       | The column naming a customer.                                                                          |
| `--rfm-date`     | yes       | The date or timestamp of each purchase.                                                                |
| `--rfm-amount`   | yes       | What each row was worth. Empty counts as 0.                                                            |
| `--rfm-order`    | no        | Rows with the same value are one order; a row without one is its own. Default: every row is one order. |
| `--rfm-as-of`    | no        | The day the segments are made for; later purchases are left out. Default: the latest date in the data. |
| `--rfm-names`    | no        | `seven` (default), `eleven` or `none`.                                                                 |

## Output

One row per customer, largest amount first:

```text
customer  last_date   days_since  orders  amount  recency  frequency  monetary  segment
bob       2024-01-05  85          1       100     1        1          4         Slipping away
ann       2024-03-20  10          2       15.3    3        3          3         Needs attention
```

The day recency was counted to, and how many rows had no customer or no
readable date, go to stderr. A modifier without `--rfm`, an `--rfm-as-of`
that is no date or an unknown `--rfm-names` word exits 1 with the reason.

## See also

- MCP: [`rfm_segments`](../mcp/tools/rfm_segments.md)
- In the app: [Customer segments (RFM)](../usage/rfm.md)
