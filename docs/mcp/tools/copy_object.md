# copy_object

Copy a cloud object, or every object under a prefix, to another location.
**Write tool**: removed by `--mcp-read-only`, and hidden from a chat profile
without **Allow writes**.

## Parameters

| Name   | Type   | Required | Description                                                           |
|--------|--------|----------|-----------------------------------------------------------------------|
| `from` | string | yes      | Source cloud URL. Ending in `/` copies the whole folder recursively.  |
| `to`   | string | yes      | Destination cloud URL. For a folder source this is the target folder. |

URL schemes: `s3://bucket/key`, `az://container/key`, `gs://bucket/key`.

## Behaviour

Source and destination may be different buckets, different accounts, or
different providers. Which path runs is decided from the URLs:

- **Same provider and bucket**: the backend copies server-side. No bytes travel
  through the server, so object size costs nothing.
- **Anything else**: the object is streamed in 8 MiB blocks into a multipart
  upload on the destination. Memory stays flat regardless of object size.

The source is never modified. A folder's shape is recreated under the target
prefix (`data/nested/b.csv` → `backup/nested/b.csv`).

## Limits and refusals

- More than **10,000 objects** in one call is refused rather than partly done.
- Copying a folder into itself or a descendant is refused.
- Copying an object onto itself is refused.

## Response

```json
{
  "from": "s3://a/data/",
  "to": "gs://b/backup/",
  "objects": 42,
  "bytes": 10485760,
  "server_side": false
}
```

`server_side` tells you which path ran.

## Credentials

The MCP server uses the ambient chain (AWS_* env, a cached SSO session, Azure
CLI login, Google application-default credentials); the in-app assistant uses
your saved cloud connections and additionally requires **Allow writing to cloud
storage** plus that connection's **Allow writes**.

## See also

- [`move_object`](move_object.md) - the same, then deletes the source
- [`delete_object`](delete_object.md)
- [`list_objects`](list_objects.md)
