# delete_object

Delete a cloud object, or every object under a prefix. **Write tool**: removed
by `--mcp-read-only`, and hidden from a chat profile without **Allow writes**.

## Parameters

| Name        | Type    | Required | Description                                                     |
|-------------|---------|----------|-----------------------------------------------------------------|
| `url`       | string  | yes      | Cloud URL to delete. Ending in `/` means the whole folder.      |
| `recursive` | boolean | no       | Required confirmation when `url` names a folder. Default false. |

## Behaviour

!!! danger "This cannot be undone"
    Unless the bucket has versioning enabled, a deleted object is gone. There is
    no trash. Prefer [`move_object`](move_object.md) to an archive prefix when
    the data might still be wanted.

The `recursive` flag exists so that a stray trailing slash cannot turn a
one-object delete into a recursive one: a folder URL without it is refused,
with a message saying what to pass.

Deleting a key that does not exist is **not** an error, matching how the
underlying object stores behave.

## Limits

More than 10,000 objects in one call is refused rather than partly done.

## Response

```json
{ "url": "s3://bucket/scratch/", "deleted": 12, "bytes": 8192 }
```

## See also

- [`move_object`](move_object.md) - the reversible alternative
- [`list_objects`](list_objects.md) - check what would be deleted first
