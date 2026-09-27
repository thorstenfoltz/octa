# Value Shapes

A shape is what a value looks like with the specifics taken out: every
digit becomes `9`, every capital letter `A`, every other letter `a`
(lower case, and scripts without a case such as CJK), everything else -
spaces, punctuation - stays as it is. `D-80331` becomes `A-99999`;
`anna@x.de` becomes `aaaa@a.aa`. Grouping a column by shape finds the
handful of values typed in the wrong format, which a "mostly text" type
check cannot.

## Switching the funnel to Shapes

Every column header funnel (see [Filter by value](filter-facets.md)) opens
listing values by default. A **Values / Shapes** switch at the top of the
popup swaps the list to shapes instead: each row shows a shape, how many
values have it, and one example. Tick shapes and press **Apply** to keep
every value that has a ticked shape - the same allow-set filter the Values
list writes, just chosen a different way. Ticking every shape, like
ticking every value, clears the filter rather than doing nothing.

The search box appears once a column holds more than 20 shapes, the same
number the Quality Report uses to decide between a **mixed** verdict and
giving up.

## The Quality Report's shape_verdict

The [Data Quality Report](data-quality-report.md) scores every text
column's shapes the same way it scores calendar coverage. A column gets
one of:

| Value        | Meaning                                                                                                                                |
|--------------|----------------------------------------------------------------------------------------------------------------------------------------|
| `consistent` | Every value shares one shape.                                                                                                          |
| `mixed`      | One shape covers at least 90% of the values, and there are at most 20 shapes, so a handful of stragglers stand out.                    |
| `na`         | Not tested: the column is empty, is not text, or has too many shapes to call anything (free text, or several formats used on purpose). |

Numeric, date and boolean columns are skipped outright: `5` and `12345`
are both perfectly good numbers with different shapes, so judging their
shape would only mislead.

## The Mixed shapes tab

Every **mixed** column's stray shapes open in their own tab beside the
report, one row per stray shape with the column it is in, how often it
occurs, and one example.

A postcode column with nineteen values shaped `A-99999` and one plain
`12345` scores **mixed**, and that one row is exactly what the Mixed
shapes tab lists - the kind of stray value a null count or a type check
would never catch.

## From the command line and MCP

```bash
octa --shapes customers.csv --shapes-column postcode
```

prints the same shapes, most common first, with a count and an example
for each. See [`--shapes`](../cli/shapes.md). The assistant's
`value_shapes` tool does the same; see
[`value_shapes`](../mcp/tools/value_shapes.md).

## See also

- [Filter by value](filter-facets.md) for the funnel popup this switch
  lives in.
- [Data Quality Report](data-quality-report.md) for the `shape_verdict`
  column and the Mixed shapes tab.
