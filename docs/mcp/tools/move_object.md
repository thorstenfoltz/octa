# move_object

Move a cloud object, or every object under a prefix, to another location.
**Write tool**: removed by `--mcp-read-only`, and hidden from a chat profile
without **Allow writes**.

## Parameters

| Name   | Type   | Required | Description                                                           |
|--------|--------|----------|-----------------------------------------------------------------------|
| `from` | string | yes      | Source cloud URL. Ending in `/` moves the whole folder recursively.   |
| `to`   | string | yes      | Destination cloud URL. For a folder source this is the target folder. |

## Behaviour

Object stores have no rename, so this is genuinely **copy then delete**, with
the same server-side / streamed split as [`copy_object`](copy_object.md).

The delete only runs after every copy has succeeded. An interrupted move
therefore leaves the source intact and some destination objects already
written, which is the recoverable direction: re-running it finishes the job.

## Limits and refusals

Identical to [`copy_object`](copy_object.md): 10,000 objects per call, no
moving a folder into itself.

## Response

```json
{ "from": "s3://a/old/", "to": "s3://a/archive/", "objects": 7, "bytes": 4096, "server_side": true }
```

## See also

- [`copy_object`](copy_object.md) - when the source should survive
- [`delete_object`](delete_object.md)
