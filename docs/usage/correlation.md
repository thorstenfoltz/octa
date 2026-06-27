# Correlation

Measure how strongly a table's numeric columns move together. Open it via
**Analyse → Correlation...**, choose a method, and press **Compute**. The
result opens in a **new detached tab**, so your original table is never
changed. It runs on the table as you currently see it, including unsaved edits.

## Methods

- **Pearson** measures *linear* association: do two columns rise and fall
  together in a straight-line way.
- **Spearman** measures *monotonic* association by correlating the value
  ranks, so it picks up relationships where one column consistently rises or
  falls with the other, even when the trend is not perfectly straight.

If you are unsure, start with Pearson; switch to Spearman when the relationship
looks curved or when you have ordinal data.

## Reading the result

Every numeric column is correlated with every other numeric column. The result
is a square table: the first column, `variable`, lists each column name, and
there is one further column per variable. Each cell holds a correlation
coefficient:

- **+1**: the two columns move together perfectly.
- **0**: no linear (or monotonic) relationship.
- **-1**: they move in perfectly opposite directions.

The diagonal is always 1 (a column correlated with itself). A pair with too few
overlapping numeric values, or with no variation in one of the columns, is left
blank. Non-numeric columns are skipped automatically, so you do not need to
select columns by hand.

Because the result is a normal table, you can sort it, copy it, colour-mark
extreme values with [conditional formatting](conditional-formatting.md), or
export it like any other tab.

## See also

- [Summary](summary.md) for per-column descriptive statistics.
- [Chart](chart.md) to plot a relationship you spotted in the matrix.
