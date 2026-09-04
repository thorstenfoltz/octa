# `--check-references`

List the child rows whose foreign key has no matching parent, and **exit 1 when
there are any** - so it works as a CI gate, like
[`--validate-schema`](validate-schema.md) and [`--check`](check.md).

```sh
octa --check-references customers.csv --parent-column id \
     --child-file orders.csv --child-column customer_id
```

Omit `--child-file` to check a **self-reference**, such as a `manager_id`
pointing at `id` in the same table.

## Flags

| Flag                      | Required? | Description                                |
|---------------------------|-----------|--------------------------------------------|
| `--check-references`      | yes       | The **parent** file.                       |
| `--parent-column`         | yes       | The parent's key column.                   |
| `--child-file`            | no        | The child file. Omit for a self-reference. |
| `--child-column`          | yes       | The child's foreign-key column.            |
| `--table-a` / `--table-b` | no        | For multi-table sources.                   |

## Output

One row per offending value, on stdout:

```text
child        key_value  rows
customer_id  9          2
```

The summary goes to **stderr**, so a pipe stays parseable:

```text
2 child row(s) across 1 value(s) have no parent.
note: 1 child row(s) have no key at all, which is not an orphan
```

The list is capped at 500 distinct values; the counts in the summary stay exact
however many there are.

## Two conventions worth knowing

- **A null or empty key is not an orphan.** In every relational database a null
  foreign key means "no parent", not "a parent that vanished". Those rows are
  counted and reported separately, and they do not fail the gate.
- **Keys are compared as trimmed text.** That is what makes `1` match `1` when
  one side came from a CSV and the other from a database.

## Exit codes

| Code | Meaning                                       |
|------|-----------------------------------------------|
| `0`  | Every child key with a value has a parent.    |
| `1`  | Orphans found, or the file could not be read. |
