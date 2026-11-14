# `--classify`

Ask TypeSafe AI's Jev model one question about every row and print the table
to stdout with the answer and its certainty as two new columns. The input file
is never modified.

```
octa --classify QUESTION --classify-columns COLS FILE
     [--classify-category NAME[=DESCRIPTION]]... [--classify-levels LEVELS]
     [--classify-name NAME] [-f tsv|json|csv]
```

!!! warning "This sends your data to TypeSafe AI"
    The cells of `--classify-columns` go to TypeSafe AI, whose servers are in
    the United States only. The action is refused until it is switched on:
    tick the box in **Settings > Classify with Jev**, or put `enabled = true`
    under `[jev]` in `settings.toml`, or set `OCTA_JEV_ENABLED=1`. The key
    comes from `TYPESAFE_API_KEY`, else from the one saved in Settings.

## The kind of answer

The flags you give decide it:

| You give              | The answer is                      | New columns                       |
|-----------------------|------------------------------------|-----------------------------------|
| `--classify-category` | one of the categories              | `category`, `category_confidence` |
| `--classify-levels`   | a grade on that scale, as a number | `score`, `score_confidence`       |
| neither               | yes or no, as true or false        | `answer`, `answer_probability`    |

Giving both is an error.

| Flag                  | Default   | What it does                                                             |
|-----------------------|-----------|--------------------------------------------------------------------------|
| `--classify-columns`  | none      | Comma-separated columns whose cells are sent. Required.                  |
| `--classify-category` | none      | One category as `NAME` or `NAME=DESCRIPTION`. Repeat it, 2 to 255 times. |
| `--classify-levels`   | none      | Comma-separated levels of a scale, lowest first, 2 to 10.                |
| `--classify-name`     | see above | Name of the answer column; it and its companion must not exist yet.      |

The four `--classify-*` flags only work together with `--classify`.

Each distinct combination of values is sent once. A row whose chosen cells are
all empty is not sent, and a value that got no answer leaves its rows empty.
One line on stderr says how many values were sent, how many input tokens the
server counted, and how many values failed.

## Examples

```
octa --classify "What is this message about?" --classify-columns message \
     --classify-category "billing=Invoices and payments" \
     --classify-category "delivery=Shipping and returns" \
     --classify-category other tickets.csv

octa --classify "Is this a complaint?" --classify-columns subject,message tickets.csv -f csv

octa --classify "How satisfied is the customer?" --classify-columns feedback \
     --classify-levels "unhappy,neutral,happy" --classify-name mood survey.parquet
```

## See also

- In the app: [Classify with Jev](../usage/classify-with-jev.md)
- MCP: [`classify_rows`](../mcp/tools/classify_rows.md)
