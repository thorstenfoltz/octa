# `detect_pii`

Scan columns for personal data and report likely matches with a confidence
score. Read-only. Each column is scored on two clues: its **header** (email,
name, gender, country, birthdate, ip, ...) and its **values** (email, phone,
IP, credit card, IBAN, SSN, date, postal-code shapes), so header-only fields
like names or country are found while plain number columns are not.

## When to use

- Finding sensitive columns before sharing or exporting a file.
- Driving an anonymisation step: the response includes suggested rules.

## Input schema

| Parameter     | Type    | Required? | Default      | Description                                          |
|---------------|---------|-----------|--------------|------------------------------------------------------|
| `path`        | string  | no\*      | (no default) | Path to the file (omit when `open_tab` is set)       |
| `open_tab`    | string  | no        | (no default) | Operate on an open GUI tab (`@active` or tab name)   |
| `table`       | string  | no        | (no default) | Specific table for multi-table sources              |
| `sample_rows` | integer | no        | `500`        | Rows sampled per column for pattern matching         |

\* `path` or `open_tab` is required.

## Response shape

```json
{
  "findings": [
    { "column": "email", "kind": "email", "confidence": 1.0,
      "by_name": true, "value_match": 1.0 }
  ],
  "suggested_rules": [
    { "columns": [1], "strategy": { "type": "hash", … }, "new_column": null }
  ]
}
```

`kind` is one of `email`, `phone`, `ip_address`, `credit_card`, `iban`,
`ssn`, `name`, `gender`, `country`, `birth_date`, `postal_code`, `address`.
`confidence` (0..1, reported when >= 0.5): `value_match >= 0.6` ->
`value_match` (+0.2 if `by_name`); else if `by_name` -> `0.6 + 0.4 *
value_match`; else `value_match`. `suggested_rules` is ready to pass to the
[`anonymize`](anonymize.md) tool (all detected columns default to a full
hash).

## Example call

```json
{
  "name": "detect_pii",
  "arguments": { "path": "/data/customers.csv" }
}
```

## See also

- [`anonymize`](anonymize.md): apply the suggested (or custom) masking
  rules.
