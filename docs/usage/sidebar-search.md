# Sidebar Search

A database connection can hold hundreds of schemas and thousands of tables,
and a bucket can hold far more objects than anyone scrolls through. The
**Databases** and **Cloud** sections of the sidebar each have a search box
under their header.

## Filter as you type

Type part of a name, in any case. The tree narrows at once to the entries
that match:

- A table, object or folder whose name matches stays visible.
- A schema, catalog or folder with a match somewhere below it opens by
  itself, so the match is on screen without clicking.
- A schema or folder whose own name matches shows everything inside it.
- Connections always stay listed, because a search starts from them.

This only looks at what the tree has already loaded, the parts you have
opened before. It needs no network, so it is instant.

## Search all

Press <kbd>Enter</kbd> in the search box, or click **Search all**, to look
through the parts you have not opened yet. Octa walks the connections you
have expanded in the background:

- **Databases**: every table of every expanded connection (and of every
  catalog, for Snowflake, Databricks and BigQuery). Most engines answer with
  one catalogue query per connection or per catalog, so this takes seconds
  even on a warehouse with hundreds of schemas. BigQuery, Athena, and a
  catalog without an `information_schema` (a Databricks `hive_metastore`)
  are walked schema by schema instead, which stops after 500 schemas.
- **Cloud**: every file and folder below the root of every expanded
  connection. For a connection to a whole account, it searches inside the
  buckets you have opened. It stops after 10,000 objects. A search matches
  names; type a `/` and it matches the whole path instead, so `2024/sales`
  finds everything in a `sales` folder under `2024`.

While it runs, a spinner shows under the box with a **Cancel** button next to
it. Cancel stops the search at once, including a long walk through a big
bucket, and drops what it had found so far.

The results appear as a list under the search box, each with where it lives.
Click a table or file to open it, exactly as clicking it in the tree would.
Click a folder (shown with a trailing `/`) and the tree opens down to it. When the walk
stopped at its limit the list says so; type more of the name to narrow it.

Only expanded connections are searched. Opening a connection is what
connects to it, and a connection you have not opened may need a sign-in, so
the search never does that behind your back. A connection that fails is
reported, and does not hide what the others found.

Changing the text in the box clears the result list, and the clear button
next to the box empties both.
