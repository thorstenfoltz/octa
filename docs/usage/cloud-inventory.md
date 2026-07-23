# Cloud Inventory

List everything under a bucket or folder into a table, without opening
any of it.

Right-click a connection or a folder in the
[cloud sidebar](cloud-storage.md) and choose **List contents as
table...**. The listing is recursive and lands in a detached tab with
one row per object:

| Column      | Meaning                                       |
|-------------|-----------------------------------------------|
| `path`      | Full key of the object inside the bucket      |
| `name`      | File name (last path segment)                 |
| `extension` | Lower-cased file extension, if any            |
| `size`      | Object size in bytes                          |
| `modified`  | Last-modified timestamp (UTC)                 |
| `etag`      | Provider ETag / content hash, when available  |
| `version`   | Object version id, when versioning is enabled |

Notes:

- The listing caps at **100,000 objects**; a banner tells you when the
  cap was hit, so you know the inventory is partial.
- Run it on a whole connection or scoped to the folder you clicked.
  For an account-level connection, run it on a bucket (or deeper), not
  on the account root.
- The result is a normal table: filter it, chart it, run SQL over it,
  or save it like any other data. Handy for answering "what is actually
  in this data lake, and how big is it?" without a console.

From an agent, the MCP tool `list_objects` does the same when called
with `recursive: true` (same 100,000-object cap; see the
[MCP docs](../mcp/index.md)).
