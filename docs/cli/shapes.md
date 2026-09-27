# `--shapes`

What a column's values look like, with the specifics taken out: digits
become `9`, capital letters `A`, other letters `a`, punctuation stays.
`D-80331` becomes `A-99999`. One row per shape, most common first, so
a handful of values typed in the wrong format stand out beside everything
that matches.

```sh
octa --shapes customers.csv --shapes-column postcode
```

## Flags

| Flag              | Required? | Description      |
|-------------------|-----------|------------------|
| `--shapes`        | yes       | The file.        |
| `--shapes-column` | yes       | Column to shape. |

## Output

```text
shape   count  example
A-99999 118    D-10115
A99999  3      D80331
```

Values longer than 24 characters shorten runs of four or more identical
shape characters as `a(12)` rather than printing them out in full. Empty
cells are not a shape; the count of shapes and of empty cells goes to
stderr.

See [Value Shapes](../usage/value-shapes.md).
