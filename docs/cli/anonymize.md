# `--anonymize`

Mask or scramble sensitive columns of a file for sharing, printing the
sanitised table to stdout. The input file is **never** modified.

```
octa --anonymize SPEC FILE [-f tsv|json|csv]
```

`SPEC` is a path to a JSON file describing the rules, an optional shared salt,
and an optional `output` mode; columns are addressed by name. `FILE` is the data
file to read.

## How it works

Every strategy keys off one deterministic digest, `HASH(salt + value)`, so the
same value plus the same salt always produces the same output. Duplicate values
stay linked (the result is still join-able) and a later re-run with the same
salt reproduces the earlier export. A non-empty salt makes the output
non-guessable. Null and empty cells pass through unchanged.

## Spec file

```json
{
  "salt": "s3cr",
  "output": "new_columns",
  "rules": [
    { "columns": "email", "strategy": { "type": "hash", "algo": "sha256" } },
    { "columns": "phone", "strategy": { "type": "partial_mask", "keep": "last", "count": 4, "mask_char": "*" } },
    { "columns": ["first", "last"], "new_column": "person_id", "strategy": { "type": "hash", "algo": "blake3", "length": 16 } }
  ]
}
```

`columns` is one name or an array of names. With a hash strategy and two or more
names the values are combined into one new column called `new_column`. `output`
is `in_place` (default, overwrite) or `new_columns` (keep originals, append).

### Strategies

| `type`         | Fields                                                  | Behaviour                                            |
|----------------|---------------------------------------------------------|------------------------------------------------------|
| `hash`         | `algo` (`sha256`/`blake3`), `length` (optional)         | Hex digest, full 64 chars unless `length` truncates. Stable and join-able. |
| `partial_mask` | `keep` (`first`/`last`), `count`, `mask_char`, `mask_len` (optional) | Keep N characters, mask the rest (`***-***-1234`). Set `mask_len` to a fixed number of mask characters so every output has the same length and the original length stops leaking. |
| `redact`       | `token` (`{ "fixed": "[REDACTED]" }` or `"null"`)       | Replace the whole value with a token or a null cell. |
| `fake`         | `kind` (`name`/`email`/`city`/`company`/`phone`/`uuid`) | Deterministic synthetic data of the chosen kind.     |

`sha256` and `blake3` both give a 64-character digest; SHA-256 is the familiar
standard, BLAKE3 is faster on large files. Omit `length` for the full digest.

## Examples

```
octa --anonymize spec.json data.csv
octa --anonymize spec.json data.parquet -f json
```

Redirect to capture the result:

```
octa --anonymize spec.json data.csv -f csv > shared.csv
```

## See also

- [Anonymise Columns](../usage/anonymize-columns.md): the GUI dialog for the
  same engine.
- The [`anonymize`](../mcp/tools/anonymize.md) MCP tool (writes a file rather
  than printing).
