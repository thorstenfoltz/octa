# Value Shapes

A shape is what a value looks like with the specifics taken out: every
digit becomes `9`, every capital letter `A`, every other letter `a`
(lower case, and scripts without a case such as CJK), and everything else,
spaces and punctuation, stays as it is. `D-80331` becomes `A-99999`.
Grouping a column by shape finds the handful of values typed in the wrong
format, which a "mostly text" type check cannot.

## Words or Exact

Letter by letter, names and words turn into long rows of `Aaaaa aaaaa`
that are hard to read and differ with every length. So by default a shape
goes **by word**: a run of letters of one kind is written once.

| Value           | Words         | Exact           |
|-----------------|---------------|-----------------|
| `Anna Schmidt`  | `Aa Aa`       | `Aaaa Aaaaaaa`  |
| `anna schmidt`  | `a a`         | `aaaa aaaaaaa`  |
| `McDonald`      | `AaAa`        | `AaAaaaaa`      |
| `D-80331`       | `A-99999`     | `A-99999`       |
| `anna@x.de`     | `a@a.a`       | `aaaa@a.aa`     |
| `INV-2026-0042` | `A-9999-9999` | `AAA-9999-9999` |

Digits keep their count either way, since that is what tells a postcode,
a phone number or an invoice number apart. Words puts every capitalised
name into one shape, so the one written in lower case stands out. Exact
tells `DE` from `DEU`, for codes where the number of letters matters.

The **Words / Exact** switch sits under **Values / Shapes** in the funnel
and in the column filter window. Octa remembers the choice, and the
Quality Report uses it too.

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

## Command line and MCP

```bash
octa --shapes customers.csv --shapes-column postcode
```

prints the same shapes, most common first, with a count and an example
for each, by word; add `--shapes-exact` for letter by letter. See
[`--shapes`](../cli/shapes.md). The assistant's `value_shapes` tool does
the same (`exact: true` for letter by letter); see
[`value_shapes`](../mcp/tools/value_shapes.md).

## See also

- [Filter by value](filter-facets.md) for the funnel popup this switch
  lives in.
- [Data Quality Report](data-quality-report.md) for the `shape_verdict`
  column and the Mixed shapes tab.
