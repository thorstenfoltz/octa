# `--reference`

Convert currency at the row's date, express amounts in the prices of another
month, or look up places and region keys, and print the table with the new
columns right after their source. Never modifies the file. The same engine as
**Data > Reference data...**; see
[Reference data](../usage/reference-data.md).

The data is public and downloaded on request. Each download must be switched
on first, in **Settings > Online data** or in `settings.toml`:

```toml
[reference.exchange_rates]
enabled = true
```

The ids are `exchange_rates`, `inflation`, `postcodes`, `eu_regions` and
`de_postcode_municipality`.

```sh
octa --reference invoices.csv --ref-mode currency --ref-amount amount --ref-from currency --ref-to USD --ref-date invoice_date
octa --reference invoices.csv --ref-mode currency --ref-amount amount --ref-from EUR --ref-to JPY --ref-date latest
octa --reference rents.csv --ref-mode inflation --ref-amount rent --ref-date month --ref-country DE
octa --reference prices.csv --ref-mode inflation --ref-amount price --ref-date 2015-01 --ref-country Germany
octa --reference customers.csv --ref-mode places --ref-key plz --ref-country DE --ref-outputs place,kreis,nuts3
octa --reference regions.csv --ref-mode places --ref-key code --ref-key-kind nuts
```

## Flags

| Flag                  | Mode                | Description                                                                                                       |
|-----------------------|---------------------|-------------------------------------------------------------------------------------------------------------------|
| `--reference`         | all                 | The file. Required.                                                                                               |
| `--ref-mode`          | all                 | `currency`, `inflation` or `places`. Required.                                                                    |
| `--ref-amount`        | currency, inflation | The amount column. Required.                                                                                      |
| `--ref-from`          | currency            | The amount's currency: a column, or one code (`EUR`) or name (`Euro`). Required.                                  |
| `--ref-to`            | currency            | The currency to convert to, a code or a name. Required.                                                           |
| `--ref-date`          | currency, inflation | Currency: the date column, or `latest` (default). Inflation: a column, or one date or `YYYY-MM`. Required.        |
| `--ref-country`       | inflation, places   | A column, or one ISO code (`XM` is the euro area). Required for inflation, postcodes and municipality codes.      |
| `--ref-target`        | inflation           | The month to express amounts in, `YYYY-MM`. Default: the newest month every country has.                          |
| `--ref-round`         | currency, inflation | `usual` (currency default: the currency's usual decimals), `none`, or a number of decimals (inflation default 2). |
| `--ref-currency-mark` | currency            | `code` (default) or `symbol` in the currency column. A symbol shared by several currencies stays a code.          |
| `--ref-key`           | places              | The column holding the key. Required.                                                                             |
| `--ref-key-kind`      | places              | `postcode` (default), `municipality`, `kreis` or `nuts`.                                                          |
| `--ref-outputs`       | places              | Comma-separated outputs. Default: all whose source is switched on.                                                |
| `--ref-first`         | places              | The first match only, alphabetically, instead of all joined.                                                      |

The outputs are `place`, `region`, `latitude`, `longitude`, `postcode`,
`municipality`, `municipality_name`, `kreis`, `kreis_name`, `nuts1`,
`nuts1_name`, `nuts2`, `nuts2_name`, `nuts3` and `nuts3_name`.

A value that names a column of the file is that column; anything else is
taken as a fixed code, so `--ref-from currency` reads the column `currency`
and `--ref-from EUR` means euros on every row.

`--ref-round usual` is for currency only. A missing required flag, or a word
that is not one of the choices, exits 1 with the reason.

## Output

The whole table with the new columns right after their source. For
`--ref-mode inflation --ref-amount amount --ref-date date --ref-country DE
--ref-target 2026-01` (made-up index values):

```text
amount  amount_in_2026-01  amount_index_month  date
100     132                2015-01 to 2026-01  2015-01-15
```

Currency adds `<amount>_<TO>`, `<amount>_<TO>_currency` and
`<amount>_<TO>_rate_date`; places adds one `<key>_<output>` column per output.
How many rows were found goes to stderr (`1 of 1 rows found`).

## Downloads and exit codes

Before the lookup, Octa downloads what is due for the switched-on sources.

- A source that is **switched off** exits 1 and names the key to set, such as
  `[reference.inflation]`.
- A **failed download** prints a `warning:` on stderr and the run carries on
  with the data stored earlier. It exits 1 only when nothing is stored for
  what was asked.

## See also

- MCP: [`reference_data`](../mcp/tools/reference_data.md)
- In the app: [Reference data](../usage/reference-data.md)
