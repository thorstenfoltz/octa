# `check_rules`

Check a table's values against a TOML rules file and report which rules
failed. Read-only, so it stays available under `--mcp-read-only`.

## Parameters

| Name         | Type   | Meaning                                           |
|--------------|--------|---------------------------------------------------|
| `path`       | string | File to check. May be a cloud URL.                |
| `open_tab`   | string | Use an open GUI tab instead (name, or `@active`). |
| `table`      | string | Sheet or table name for multi-table sources.      |
| `rules_path` | string | The TOML rules file.                              |

The rules file is the same one the **Data -> Data validation...** dialog
saves and loads, and the same one `octa --check` reads. Kinds are
`not_null`, `unique`, `range`, `regex` and `max_length`; a rule without
a `column` applies to every column.

## Response

```json
{
  "passed": false,
  "rules_checked": 3,
  "violations": [
    { "rule": "unique", "column": "order_id", "failures": 4, "samples": ["17", "17", "42"] }
  ],
  "unknown": ["region"]
}
```

`unknown` lists rules that could not run, because they name a column the
table does not have. **`passed` is false whenever `unknown` is
non-empty**, even with no violations: a rules file whose columns have
been renamed would otherwise look like a clean run over checks that were
silently skipped.

See [Data Validation](../../usage/data-validation.md).
