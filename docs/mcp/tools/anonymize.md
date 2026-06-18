# `anonymize`

**Mask / scramble sensitive columns** of a tabular file and write the sanitised
result. A "prepare for sharing" pass: pick columns and a per-column strategy,
and Octa rewrites them deterministically.

This is a **write** tool, so it is removed under `--mcp-read-only` (alongside
`write_table`, `edit_table`, `convert`, and `transform_columns`).

## When to use

- Producing a shareable extract with emails / names / phone numbers masked.
- Keeping data join-able after masking (the same value + salt always maps the
  same way, so duplicates stay linked and re-runs re-join).

## Input schema

| Parameter     | Type     | Required? | Default          | Description                                                                     |
|---------------|----------|-----------|------------------|---------------------------------------------------------------------------------|
| `path`        | string   | yes       | (no default)     | Path to the source file                                                         |
| `rules`       | object[] | yes       | (no default)     | Rules: `{ "columns": NAME or [NAMES], "strategy": {...}, "new_column"?: NAME }` |
| `salt`        | string   | no        | `""`             | Shared salt for all rules; makes output non-guessable                           |
| `output`      | string   | no        | `in_place`       | `in_place` overwrites the columns; `new_columns` keeps originals and appends    |
| `output_path` | string   | no        | overwrite `path` | Where to write the result; format follows its extension                         |
| `unlimited`   | bool     | no        | `false`          | Lift the 5,000,000-row file-loader cap so every row is rewritten                |

`columns` is one column name or an array of names. With a **hash** strategy and
two or more names, the values are combined into one new column named
`new_column` (a pseudonymous key). For mask / redact / fake, multiple names
apply the strategy to each column.

Database files (SQLite / DuckDB / GeoPackage) are not valid sources or targets.
Null and empty cells always pass through unchanged.

### Strategies

Each rule's `strategy` is one of:

| `type`         | Fields                                                               | Behaviour                                                                                                                                                                        |
|----------------|----------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `hash`         | `algo` (`sha256`/`blake3`), `length` (optional)                      | Hex digest, full 64 chars unless `length` truncates it. Stable + join-able.                                                                                                      |
| `partial_mask` | `keep` (`first`/`last`), `count`, `mask_char`, `mask_len` (optional) | Keep N characters, mask the rest (`***-***-1234`). Set `mask_len` to a fixed number of mask characters so every output is the same length and the original length stops leaking. |
| `redact`       | `token` (`{ "fixed": "[REDACTED]" }` or `"null"`)                    | Replace the whole value with a token or a null cell.                                                                                                                             |
| `fake`         | `kind` (`name`/`email`/`city`/`company`/`phone`/`uuid`)              | Deterministic synthetic data of the chosen kind.                                                                                                                                 |

`sha256` and `blake3` both produce a 64-character digest; SHA-256 is the
familiar standard, BLAKE3 is faster on large files. Omit `length` for the full
digest, or set it to keep a shorter prefix (shorter = small collision chance).

## Response shape

```json
{
  "rows_written": 1000,
  "columns_anonymized": 2,
  "output": "/tmp/people_shared.csv"
}
```

## Example call

```json
{
  "name": "anonymize",
  "arguments": {
    "path": "/tmp/people.csv",
    "output_path": "/tmp/people_shared.csv",
    "salt": "s3cr",
    "rules": [
      { "columns": "email", "strategy": { "type": "hash", "algo": "sha256" } },
      { "columns": "phone", "strategy": { "type": "partial_mask", "keep": "last", "count": 4, "mask_char": "*" } },
      { "columns": ["first", "last"], "new_column": "person_id", "strategy": { "type": "hash", "algo": "sha256", "length": 16 } }
    ]
  }
}
```

## See also

- [Anonymise Columns](../../usage/anonymize-columns.md): the GUI dialog and CLI
  `--anonymize` flag for the same engine.
- [`write_table`](write_table.md): write inline rows to a new file.
