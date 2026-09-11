# Compare with a Database Table or Cloud Object

<!-- SCREENSHOT: db-compare-dialog.png: The Compare with database table dialog: connection combo showing a saved connection, schema `public` and table `orders` filled in, and two key columns ticked in the key-column picker below. -->

**Analyse → Compare with database or cloud...** diffs the table you have
open against a table on a saved [database
connection](database-connections.md), or against an object in [cloud
storage](cloud-storage.md).

This is the question you have after a load: *did what I sent actually
land?* Until now answering it meant exporting the other side to a file
first and diffing the two files.

## What it does

Rows are matched on the **key columns** you pick, the way a join would,
and the result opens in a detached tab in the standard compare shape:

| Column            | Meaning                                            |
|-------------------|----------------------------------------------------|
| `status`          | `only_in_a`, `only_in_b`, `changed_a`, `changed_b` |
| `changed_columns` | Which columns differ, for the changed pairs        |
| (data columns)    | The row itself                                     |

"A" is your open tab, "B" is the database table. So `only_in_a` is in
your file but not the warehouse, and `only_in_b` is the other way round.

There is no new comparison logic here: this is the same
`compare_join` engine behind
[`--diff --diff-mode join`](../cli/diff.md), given a database for one
side instead of a file.

## Comparing against a cloud object

Set **Compare against** to **Cloud object** and paste the object's URL:

| Provider             | URL form                  |
|----------------------|---------------------------|
| Amazon S3, MinIO, R2 | `s3://bucket/key.parquet` |
| Azure Blob Storage   | `az://container/blob`     |
| Google Cloud Storage | `gs://bucket/key`         |

The object is downloaded to a temporary file and read like any local file,
so every format Octa opens works here. Credentials come from a saved cloud
connection covering the URL, otherwise from your ambient cloud login
(`AWS_*` environment variables, a cached SSO session, `az login`, or Google
application default credentials).

The same URLs work on the command line, where a cloud URL is accepted
anywhere a file is:

```bash
octa --diff local.parquet s3://bucket/exports/day.parquet \
     --diff-mode join --diff-on customer_id
```

## Using it

1. Open the file you want to check.
2. **Analyse → Compare with database or cloud...**
3. Pick the connection, then fill in schema and table. Leave **Catalog**
   empty unless the connection is Snowflake, Databricks or BigQuery. An
   empty schema uses the connection's own database.
4. Tick the **key columns** that identify a row. The Compare button
   stays disabled until you have named a table and at least one key, and
   its tooltip says which is missing.

The read runs on a worker thread, so the window stays responsive while a
slow connection answers.

## On the command line

```bash
octa --diff orders.csv \
     --diff-db warehouse --diff-db-table public.orders \
     --diff-mode join --diff-on id
```

`--diff-db` names a saved connection, so a single file is the whole
positional input. Output is the same compare table, on stdout:

```text
status     changed_columns  id  city      amount
only_in_a                   3   Tokyo     30
only_in_b                   4   Cologne   40
changed_a  amount           2   Helsinki  20
changed_b  amount           2   Helsinki  99
mode join - unchanged: 1 - changed: 1 - only in A: 1 - only in B: 1
```

Columns are aligned here for readability; the real output is
tab-separated (or CSV / JSON with `-f`), and the summary line goes to
standard error so a pipe carries only the table. The blank
`changed_columns` cells are the rows that exist on one side only.

That makes it usable as a load check in a pipeline.

## Over MCP

`diff_tables` takes a `b_db` object instead of `path_b`:

```json
{
  "path_a": "orders.csv",
  "b_db": { "connection": "warehouse", "table": "public.orders" },
  "mode": "join",
  "on": ["id"]
}
```

## Ceilings

- **Only loaded rows are compared.** Both sides are read under the usual
  row cap, so on a large table the comparison covers its first rows. The
  result tab says so in a banner when either side hit the cap; raise the
  cap in Settings if you need the whole thing.
- **The comparison is a snapshot.** Nothing locks the table, so rows
  written while the read is in flight may or may not appear.
- Either side can be a database, so table-versus-table across two
  connections works the same way from the CLI.

<!-- TODO screenshot: the Compare with database table dialog with a
     connection picked, schema/table filled in and two key columns
     ticked. Listed in docs/assets/screenshots/INDEX.md. -->
