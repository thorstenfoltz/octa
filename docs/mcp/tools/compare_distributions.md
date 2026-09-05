# `compare_distributions`

Compare the distributions of **two columns** and say whether they look like the
same population. Read-only analytics (stays available under `--mcp-read-only`).

## When to use

- "Did this month's data change shape?" before trusting a downstream number.
- Compare the same column across two files, or two columns of one file.
- Check a treatment group against a control without writing the statistics.

## How the test is chosen

From the data, not from a parameter:

- **Numeric** columns use a two-sample **Kolmogorov-Smirnov** test, which
  compares the whole shape rather than one summary of it.
- **Anything else** is compared as categories with a **chi-square test of
  homogeneity**.

## Input schema

| Parameter    | Type   | Required? | Default      | Description                              |
|--------------|--------|-----------|--------------|------------------------------------------|
| `path`       | string | yes*      | (no default) | First file (omit when `open_tab` is set) |
| `open_tab`   | string | no        | (none)       | Open GUI tab name, or `@active`          |
| `table`      | string | no        | first table  | Table within a multi-table first source  |
| `column`     | string | yes       | (no default) | Name of the first column                 |
| `path_b`     | string | no        | first source | Second file                              |
| `open_tab_b` | string | no        | first source | Open tab holding the second column       |
| `table_b`    | string | no        | first table  | Table within a multi-table second source |
| `column_b`   | string | no        | `column`     | Name of the second column                |
| `unlimited`  | bool   | no        | `false`      | Lift the streaming row cap               |

\* One of `path` or `open_tab` is required.

## Response

```json
{
  "first": "amount",
  "second": "amount",
  "headline": "The second sample skews 12% higher.",
  "headline_id": "higher",
  "verdict": "different",
  "test": "kolmogorov_smirnov",
  "statistic": 0.184,
  "p_value": 0.0031,
  "degrees_of_freedom": null,
  "first_rows": 4210,
  "second_rows": 3980
}
```

`headline` is a plain sentence, because `D = 0.184, p = 0.003` is not an answer
anyone can act on. `headline_id` is the stable machine form: `same`, `higher`,
`lower`, `category_moved` or `different`. `verdict` is `same` or `different` at
p = 0.05. `degrees_of_freedom` is null for the Kolmogorov-Smirnov test.

For categories the headline names the category whose share moved most.

## When the test does not apply

The response is `{"first": ..., "second": ..., "skipped": "na_<reason>"}`:

| Reason                   | Meaning                                                  |
|--------------------------|----------------------------------------------------------|
| `na_too_few_values`      | Fewer than 20 usable values in one of the columns.       |
| `na_too_many_categories` | More than 50 distinct values: free text, not categories. |
| `na_too_few_categories`  | Not enough categories left after pooling rare ones.      |

Categories too rare for a chi-square cell are **pooled into one `(other)`
bucket rather than dropped**, so their rows still count towards the totals.
Nulls are excluded: a column getting emptier is a completeness finding, not a
distribution one.
