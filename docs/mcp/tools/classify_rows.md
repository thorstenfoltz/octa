# `classify_rows`

**Label every row** of a tabular file with TypeSafe AI's Jev decision model
and write the result. Adds the answer and its certainty as two new columns.
Same engine as **Columns > Classify with Jev...** in the app and
`octa --classify`.

This is a **write** tool, so it is removed under `--mcp-read-only`.

!!! warning "This sends data to TypeSafe AI"
    The cells of `columns` go to TypeSafe AI, whose servers are in the United
    States only. The tool is **refused until the user has switched Classify
    with Jev on** and set a key, in Settings, `settings.toml` or the
    environment. A model calling the tool cannot do that for them. See
    [Classify with Jev](../../usage/classify-with-jev.md#switching-it-on).

## When to use

- Sorting free text into categories the caller names.
- Answering one yes or no question per row.
- Grading rows on a short ordered scale.

## Input schema

| Parameter      | Type     | Required? | Default                       | Description                                                               |
|----------------|----------|-----------|-------------------------------|---------------------------------------------------------------------------|
| `path`         | string   | yes       | (no default)                  | Path to the source file                                                   |
| `columns`      | string[] | yes       | (no default)                  | Names of the columns whose cells are sent                                 |
| `mode`         | string   | yes       | (no default)                  | `categories`, `yes_no` or `scale`                                         |
| `instructions` | string   | yes       | (no default)                  | The question, in plain words                                              |
| `categories`   | object[] | no        | `[]`                          | For `categories`: 2 to 255 of `{name, description}`; description optional |
| `levels`       | string[] | no        | `[]`                          | For `scale`: 2 to 10 levels, lowest first                                 |
| `new_column`   | string   | no        | `category`, `answer`, `score` | Name of the answer column; a taken name is refused                        |
| `output_path`  | string   | no        | overwrite `path`              | Where to write the result; format follows its extension                   |
| `unlimited`    | bool     | no        | `false`                       | Lift the file-loader row cap so every row is read                         |

The certainty column is `<name>_confidence`, or `<name>_probability` for
`yes_no`, and holds a number from 0 to 1. Each distinct combination of values
is sent once. Rows whose chosen cells are all empty, and values that got no
answer, stay empty. A file read only in part is refused rather than written
back over itself with fewer rows; pass `unlimited: true`. Database files
(SQLite / DuckDB / GeoPackage) are not valid sources or targets.

## Example

```json
{
  "path": "/data/tickets.csv",
  "columns": ["message"],
  "mode": "categories",
  "instructions": "What is this message about?",
  "categories": [
    { "name": "billing", "description": "Invoices and payments" },
    { "name": "other" }
  ],
  "output_path": "/data/tickets_classified.csv"
}
```

## Output

```json
{
  "rows_written": 1200,
  "new_column": "category",
  "output": "/data/tickets_classified.csv",
  "model": "jev-latest",
  "values_sent": 874,
  "failed": 0,
  "input_tokens": 41210
}
```

`failed` counts the values that got no answer; `first_error` is added when it
is not zero.

## See also

- In the app: [Classify with Jev](../../usage/classify-with-jev.md)
- Command line: [`--classify`](../../cli/classify.md)
