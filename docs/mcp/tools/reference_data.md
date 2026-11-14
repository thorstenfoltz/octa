# `reference_data`

Convert currency at the row's date, express amounts in the prices of another
month, or look up places and region keys. The same engine as the GUI's
[Reference data](../../usage/reference-data.md) and `octa --reference`.
Changes no file.

Read-only, so it stays available under `--mcp-read-only`.

The data is public and downloaded on request, and **every download is off
until the user switches it on** in **Settings > Online data**. While the
download a call needs is off, the call is refused:

```text
reference_data failed: Inflation is switched off: switch it on in Settings > Online data, or set `enabled = true` under [reference.inflation] in settings.toml
```

## Input schema

| Name            | Type            | Mode                | Meaning                                                                                       |
|-----------------|-----------------|---------------------|-----------------------------------------------------------------------------------------------|
| `path`          | string          | all                 | The file. May be a cloud URL.                                                                 |
| `open_tab`      | string          | all                 | Use an open GUI tab instead (name, or `@active`).                                             |
| `table`         | string          | all                 | Sheet or table name for multi-table sources.                                                  |
| `mode`          | string          | all                 | `currency`, `inflation` or `places`. Required.                                                |
| `amount`        | string          | currency, inflation | The amount column.                                                                            |
| `from`          | string          | currency            | The amount's currency: a column, or one code or name.                                         |
| `to`            | string          | currency            | The currency to convert to, a code or a name.                                                 |
| `date`          | string          | currency, inflation | The date column. Currency: omit or `latest` for the newest rate. Inflation: or one `YYYY-MM`. |
| `country`       | string          | inflation, places   | A column, or one country name or ISO code (`XM` is the euro area).                            |
| `target_month`  | string          | inflation           | `YYYY-MM`. Default: the newest month every country has.                                       |
| `rounding`      | string          | currency, inflation | `usual` (currency default), `none`, or a number of decimals (inflation default `2`).          |
| `currency_mark` | string          | currency            | `code` (default) or `symbol` in the currency column.                                          |
| `key`           | string          | places              | The column holding the key.                                                                   |
| `key_kind`      | string          | places              | `postcode` (default), `municipality`, `kreis` or `nuts`.                                      |
| `outputs`       | array of string | places              | Which columns to add. Default: all whose source is switched on.                               |
| `several`       | string          | places              | `all` (default, joined in one cell) or `first` (alphabetically).                              |
| `limit`         | number          | all                 | At most this many rows in the answer; `0` for all.                                            |

A value that names a column is that column; anything else is a fixed code.
The outputs are `place`, `region`, `latitude`, `longitude`, `postcode`,
`municipality`, `municipality_name`, `kreis`, `kreis_name`, `nuts1`,
`nuts1_name`, `nuts2`, `nuts2_name`, `nuts3` and `nuts3_name`.

## Response shape

For `mode: "inflation"`, `amount: "amount"`, `date: "date"`, `country: "DE"`
and `target_month: "2026-01"` (made-up index values):

```json
{
  "columns": ["amount_in_2026-01", "amount_index_month"],
  "found": 1,
  "missed": 0,
  "warnings": [],
  "table": { "schema": [], "rows": [], "row_count": 1, "truncated": false, "total_rows_available": 1 }
}
```

`table` has the usual `schema` and `rows` (shortened above) and holds only
the source column and the new ones. `found` and `missed` count
the rows that got a value and those that did not; a missed row's last column
says why (`no rate before 1960-01-01`, `unknown currency XXX`). `warnings`
holds a failed download: the answer then rests on the data stored earlier.
The call is an error only when nothing is stored for what was asked.

## See also

- Command line: [`--reference`](../../cli/reference.md)
- In the app: [Reference data](../../usage/reference-data.md)
