# Compare Distributions

**Analyse > Compare distributions...** answers one question: do these two
columns look like the same population?

It is the question behind "did something change" - last month against this
month, control against treatment, the file the vendor sent in June against the
one they sent in July. A mean and a standard deviation answer it badly: two
samples can share both and still be shaped nothing alike.

## Picking the columns

The dialog has two rows, each a tab picker and a column picker. Both start on
the tab you opened it from, because comparing two columns of one table is as
common as comparing one column across two files. Changing a tab clears the
column beside it: index 3 of one table is not index 3 of another, and quietly
comparing the wrong column would be worse than making you pick again.

**There is no test picker.** The engine chooses from what the columns hold, and
offering the choice would only let you pick wrong:

- **Numbers** are compared with a two-sample **Kolmogorov-Smirnov** test, which
  looks at the whole shape rather than at one summary of it.
- **Anything else** is compared as categories with a **chi-square test of
  homogeneity** over the shared set of values.

A column that is not numeric throughout is compared as categories, which is the
honest reading of a column that is not really numeric.

## Reading the answer

The result opens as its own tab, and the headline also appears in the status
bar so you see it before you have parsed the table:

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
p = 0.003` tells almost nobody anything, so the sentence comes first and the
number stays underneath for anyone who wants it. `verdict` is `same` or
`different` at the conventional p = 0.05.

For categories the headline names the category whose share moved most, which is
the one worth acting on, and the table carries a `degrees_of_freedom` row too.

## When it does not apply

Rather than a verdict you get a `result` row saying why:

| Value                    | Reason                                           |
|--------------------------|--------------------------------------------------|
| `na_too_few_values`      | Fewer than 20 usable values in one column.       |
| `na_too_many_categories` | More than 50 distinct values: that is free text. |
| `na_too_few_categories`  | Too few categories left after pooling rare ones. |

Two details worth knowing about the categorical test:

- **Rare categories are pooled, not dropped.** A chi-square wants at least five
  expected observations per cell; anything rarer joins one `(other)` bucket, so
  those rows still count towards the totals.
- **Nulls are left out entirely.** A column getting emptier is a completeness
  finding, and the [quality report](data-quality-report.md)'s
  `null_percentage` is where that lives.

## Elsewhere

The same comparison is on the command line as
[`--compare-distributions`](../cli/compare-distributions.md) and in the MCP
server as [`compare_distributions`](../mcp/tools/compare_distributions.md).

## See also

- [Correlation](correlation.md) asks whether two columns move together, not
  whether they are shaped alike.
- [Data drift](data-drift.md) compares whole profiles between two datasets
  rather than one column against another.
