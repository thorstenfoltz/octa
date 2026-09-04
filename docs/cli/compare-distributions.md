# `--compare-distributions`

Do two columns look like the same population?

```sh
octa --compare-distributions sales.csv --dist-column amount --dist-column-b amount_last_year
octa --compare-distributions q2.parquet --dist-column price \
     --dist-file-b q3.parquet
```

The test is chosen from the data, not by you:

- **Numbers** are compared with a two-sample **Kolmogorov-Smirnov** test, which
  looks at the whole shape rather than at one summary of it. Two samples can
  share a mean and a standard deviation and still be shaped nothing alike.
- **Anything else** is compared as categories with a **chi-square test of
  homogeneity** over the shared set of values.

A column that is not numeric throughout is compared as categories, which is the
honest reading of a column that is not really numeric.

## Flags

| Flag                      | Required? | Description                                              |
|---------------------------|-----------|----------------------------------------------------------|
| `--compare-distributions` | yes       | The first file.                                          |
| `--dist-column`           | yes       | The column to compare.                                   |
| `--dist-file-b`           | no        | A second file. Omit to compare two columns of the first. |
| `--dist-column-b`         | no        | The second column. Defaults to `--dist-column`.          |
| `--table-a` / `--table-b` | no        | For multi-table sources.                                 |

## Output

A two-column `field` / `value` table, so it reads at any width:

```text
field         value
first         amount
second        amount_last_year
headline      The second sample skews 12% higher.
verdict       different
test          kolmogorov_smirnov
statistic     0.1840
p_value       0.0031
first_rows    4210
second_rows   3980
```

**The headline is the answer; the statistic is the evidence.** `D = 0.184,
p = 0.003` tells almost nobody anything, so the sentence comes first. `verdict`
is `same` or `different` at p = 0.05.

For categories the headline names the category that moved most, and the output
carries a `degrees_of_freedom` row as well.

## When it does not apply

`verdict` is replaced by a `result` row saying why:

| Value                    | Reason                                                   |
|--------------------------|----------------------------------------------------------|
| `na_too_few_values`      | Fewer than 20 usable values in one of the columns.       |
| `na_too_many_categories` | More than 50 distinct values: free text, not categories. |
| `na_too_few_categories`  | Not enough categories left after pooling rare ones.      |

Categories too rare for a chi-square cell are **pooled into one `(other)`
bucket rather than dropped**, so their rows still count towards the totals.
Nulls are left out entirely: a column getting emptier is a completeness
finding, which is what the quality report's `null_percentage` is for.

## See also

- [`--compare-schemas`](compare-schemas.md) compares the columns, not the
  values.
- [`--drift-report`](drift-report.md) compares whole profiles between two
  datasets.
