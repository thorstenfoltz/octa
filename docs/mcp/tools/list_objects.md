# `list_objects`

List **one folder level of a cloud object store** by URL: Amazon S3 (and
S3-compatible providers), Azure Blob Storage, or Google Cloud Storage.
Read-only analytics (stays available under `--mcp-read-only`).

## When to use

- "What files are in `s3://reports/2026/`?"
- Browse a bucket before opening a file with [`read_table`](read_table.md).

## Input schema

| Parameter | Type   | Required? | Default      | Description                                                       |
|-----------|--------|-----------|--------------|-------------------------------------------------------------------|
| `url`     | string | yes       | (no default) | `s3://bucket/prefix`, `az://container/prefix`, or `gs://bucket/prefix` (empty prefix = bucket root) |

## Credentials

The MCP/CLI server authenticates with **ambient credentials**:

- **S3**: `AWS_*` environment variables or a cached SSO session.
- **Azure**: an Azure CLI login, plus `AZURE_STORAGE_ACCOUNT` (an `az://` URL
  cannot carry the storage account name).
- **GCS**: Google application-default credentials, or `GOOGLE_*` environment
  variables.

The in-app assistant additionally uses your **saved cloud connections**
(Settings > Cloud storage), so it can reach buckets configured with static
keys or per-connection sign-in.

## Response shape

```json
{
  "url": "s3://reports/2026/",
  "count": 2,
  "objects": [
    { "name": "q1", "key": "2026/q1/", "url": "s3://reports/2026/q1/", "is_folder": true, "size": null, "modified": null },
    { "name": "summary.parquet", "key": "2026/summary.parquet", "url": "s3://reports/2026/summary.parquet", "is_folder": false, "size": 81234, "modified": "2026-06-20T11:02:00+00:00" }
  ]
}
```

Open a listed file by passing its `url` as the `path` of any read tool.

## Example call

```json
{
  "name": "list_objects",
  "arguments": {
    "url": "s3://reports/2026/"
  }
}
```

## See also

- [`read_table`](read_table.md): open a listed file (pass its `url` as `path`).
- [`grep_files`](grep_files.md): grep across local files in a directory.
