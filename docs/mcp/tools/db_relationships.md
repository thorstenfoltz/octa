# `db_relationships`

Read the foreign keys a live database **declares**, so you learn how its
tables connect without reading a single row.

Its file-based sibling, [`suggest_join_keys`](suggest_join_keys.md),
infers a link from values. This one asks the server, which already
knows: somebody declared the constraints. That costs two catalog queries
and no table data, so it answers just as quickly on a warehouse where
sampling every table would not.

Read-only, and kept when the server runs with `--mcp-read-only`.

## Parameters

| Name         | Type              | Meaning                                                                                 |
|--------------|-------------------|-----------------------------------------------------------------------------------------|
| `connection` | string (required) | Saved connection name or id, from [`list_db_connections`](list_db_connections.md)       |
| `catalog`    | string            | Top namespace level on Snowflake, Databricks and BigQuery. An error on any other engine |
| `schemas`    | array of string   | Schemas to read. Empty: every schema the connection lists                               |
| `tables`     | array of string   | Draw only these `schema.table` labels. Absent: the tables taking part in a foreign key  |
| `max_tables` | integer           | Tables carried in the answer before it stops. Default 30                                |
| `measure`    | boolean           | Also read a sample of rows and score every edge. Default false                          |
| `sample`     | integer           | Rows sampled per table when `measure` is set. Default 10000                             |

## Result

```json
{
  "connection": "warehouse",
  "engine": "PostgreSQL",
  "enforced": true,
  "schemas": ["public"],
  "tables": ["public.orders", "public.customers"],
  "count": 1,
  "relationships": [
    {
      "child": "public.orders",
      "child_column": "customer_id",
      "parent": "public.customers",
      "parent_column": "id",
      "constraint": "orders_customer_id_fkey",
      "measured": false
    }
  ],
  "skipped_edges": 0,
  "truncated": false,
  "measured": false
}
```

## A declaration is not a measurement

`enforced` is the field to read before you trust an edge:

- Postgres, MySQL, SQL Server and Exasol **enforce** their foreign keys,
  so an edge from those servers is true of the rows as well.
- Redshift, Snowflake, Databricks and BigQuery **accept a declaration and
  enforce nothing**. A child value pointing at a parent that does not
  exist is entirely possible there.

`measure: true` settles it. Each edge then also carries `overlap`,
`score` and both orphan counts (`left_orphans` out of
`left_distinct_values`, `right_orphans` out of `right_distinct_values`),
computed by the same scorer
[`suggest_join_keys`](suggest_join_keys.md#how-the-score-is-calculated)
uses. A declared key is scored **child to parent**, so `left_orphans` is
the child rows pointing at a parent that does not exist, and a key that
nothing honours
shows up as orphans. It is off by default because it is the step that
reads your data.

ClickHouse has no referential constraints of any kind, so it is refused
with a pointer to `suggest_join_keys` over exported files.

## Limits

- At most 30 tables by default; `truncated` says when it stopped.
- A declared key whose other end is not drawn is counted in
  `skipped_edges` rather than reported as an edge into nowhere.
- Measuring samples 10,000 rows per table, so a clean result is strong
  evidence rather than proof.

## See also

- [Relationship map](../../usage/relationship-map.md) draws the same
  answer in the GUI.
- [`suggest_join_keys`](suggest_join_keys.md) for files, where nothing
  was declared.
