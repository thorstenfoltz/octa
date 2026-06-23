# `--detect-pii`

Scan a file for columns that look like personal data and print the findings
to stdout. Read-only.

```
octa --detect-pii FILE [--pii-sample N] [-f tsv|json|csv]
```

`--pii-sample` sets how many rows are sampled per column for pattern matching
(default `500`).

## How it works

Each column is scored on two clues: its **header** (email, name, gender,
country, birthdate, ip, ...) and its **values** (email, phone, IP, credit
card, IBAN, SSN, date, postal-code shapes). A strong value pattern alone, or
a matching header alone, reaches 0.5; together they score highest. So name,
gender and country columns are caught from the header, while a plain number
column like `salary` is left alone.

## Output

One row per finding with these columns:

| Column        | Meaning                                                                                                                           |
|---------------|-----------------------------------------------------------------------------------------------------------------------------------|
| `column`      | Column name                                                                                                                       |
| `kind`        | `email`, `phone`, `ip_address`, `credit_card`, `iban`, `ssn`, `name`, `gender`, `country`, `birth_date`, `postal_code`, `address` |
| `confidence`  | 0..1 score (reported when >= 0.5)                                                                                                 |
| `by_name`     | Whether the header matched                                                                                                        |
| `value_match` | Fraction of sampled values matching the kind's pattern                                                                            |

`by_name` and `value_match` show how the confidence was reached. Use it to
decide which columns to mask with [`--anonymize`](anonymize.md).

## Examples

```
octa --detect-pii customers.csv
octa --detect-pii customers.parquet --pii-sample 2000 -f json
```

## See also

- [Detect PII](../usage/detect-pii.md) (GUI) and the
  [`detect_pii`](../mcp/tools/detect_pii.md) MCP tool.
- [`--anonymize`](anonymize.md): mask the columns you find.
