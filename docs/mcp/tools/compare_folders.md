# `compare_folders`

Compare two or more folders file by file: which files are the same
everywhere, which differ, and which are missing from a folder. Read-only,
so it stays available under `--mcp-read-only`.

CLI mirror: [`octa --compare-folders`](../../cli/compare-folders.md).

## When to use

- Checking that a published folder or bucket matches the build that
  produced it.
- Finding what changed between two exports, or between a local folder and
  its copy in the cloud.
- Comparing three or more copies of one folder at once.

## Input schema

| Parameter   | Type             | Required? | Default                | Description                                                          |
|-------------|------------------|-----------|------------------------|----------------------------------------------------------------------|
| `folders`   | array of strings | yes       | (no default)           | Two or more local folder paths or cloud URLs.                        |
| `recursive` | boolean          | no        | `true`                 | Include subfolders.                                                  |
| `limit`     | integer          | no        | the server's row limit | Response row cap; `0` = unlimited.                                   |
| `unlimited` | boolean          | no        | `false`                | List every file instead of stopping at the file limit from Settings. |

A cloud URL is `s3://`, `az://`, `gs://`, `sftp://` or `davs://`, and the
folders may mix local and cloud.

## How files are compared

Files are matched by their **path inside each folder**. Two copies of
different sizes are different without being read. Two or more copies of
the same size are read in full and compared by a BLAKE3 hash, so equal
size alone never counts as the same. A file in one folder only is never
read.

Each folder is listed up to the Compare folders file limit from Settings
(100,000 files per folder by default). A folder that stopped there is
named in `truncated_folders`, and its comparison is incomplete;
`unlimited: true` lifts the limit.

## Response shape

The usual table payload, one row per file path, plus two fields:

```json
{
  "schema": [
    { "name": "path", "type": "Utf8" },
    { "name": "status", "type": "Utf8" },
    { "name": "in", "type": "Utf8" },
    { "name": "missing from", "type": "Utf8" },
    { "name": "size: monday", "type": "Int64" },
    { "name": "modified: monday", "type": "Timestamp(Microsecond, None)" },
    { "name": "hash: monday", "type": "Utf8" },
    { "name": "size: tuesday", "type": "Int64" },
    { "name": "modified: tuesday", "type": "Timestamp(Microsecond, None)" },
    { "name": "hash: tuesday", "type": "Utf8" }
  ],
  "rows": [
    ["new.csv", "missing", "tuesday", "monday", null, null, null, 512, "2026-10-14 09:12:00", null],
    ["sales.csv", "different", "monday, tuesday", "", 1204, "2026-10-13 18:00:00", "0b8b...", 1204, "2026-10-14 09:10:00", "27a5..."]
  ],
  "row_count": 2,
  "truncated": false,
  "total_rows_available": 2,
  "cell_truncated": false,
  "any_difference": true,
  "truncated_folders": []
}
```

Hashes are shortened here; the real ones are 64 hex characters.

| `status`    | Meaning                                                                 |
|-------------|-------------------------------------------------------------------------|
| `same`      | Every folder has the file and every copy is identical.                  |
| `different` | At least two copies differ. The file may also be missing from a folder. |
| `missing`   | Some folders lack the file, and the copies that exist are identical.    |

`any_difference` is `true` when any row is not `same`. The folder labels
in the column names are the last part of each folder's path, with more of
the path added when two would otherwise clash.

## Example call

```json
{
  "name": "compare_folders",
  "arguments": {
    "folders": ["/data/exports", "s3://acme-data/exports/"]
  }
}
```

## Sandbox and credentials

Under `octa --mcp`, a cloud URL uses the ambient credentials, like every
other MCP tool: the AWS environment or SSO, `az login` plus
`AZURE_STORAGE_ACCOUNT`, Google application-default credentials, and your
SSH agent for SFTP. WebDAV needs a saved connection, so it is reachable
only from the Assistant and the command line. In the in-app Assistant the
filesystem sandbox applies: it reaches only the buckets you saved as
cloud connections, and it cannot list local folders at all, because the
sandbox admits open files, never folders.

## See also

- [`octa --compare-folders`](../../cli/compare-folders.md), the CLI
  mirror, which exits 1 on any difference for CI gating.
- [`diff_tables`](diff_tables.md): two tables, row by row.
- [`schema_drift`](schema_drift.md): which files in one folder disagree
  about their columns.
- [`list_objects`](list_objects.md): one level of a bucket.
- In the app: [Compare folders](../../usage/compare-folders.md)
