# `query_api`

Read a saved REST/JSON API endpoint as a table.

The endpoint's authentication is applied and its pagination is walked, so the
result holds every page's rows rather than just the first. Names come from
[`list_api_connections`](list_api_connections.md).

Read-only, and kept when the server runs with `--mcp-read-only`.

## Parameters

| Name           | Type    | Required | Description                                                                         |
|----------------|---------|----------|-------------------------------------------------------------------------------------|
| `connection`   | string  | yes      | Saved connection name or id.                                                        |
| `path`         | string  | no       | Path under the endpoint's base URL, overriding the connection's own.                |
| `records_path` | string  | no       | JSON pointer to the array of rows (`/data/items`), overriding the connection's own. |
| `limit`        | integer | no       | Cap the rows in the response.                                                       |

## Returns

The same `{schema, rows, ...}` shape as [`read_table`](read_table.md), plus:

- `pages_read` — how many requests the pagination rule made.
- `note` — present only when the read stopped on a row or page limit, saying
  so, because a truncated result that looks complete is worse than a slow one.

## Notes

`path` is **joined under the saved base URL**, never used as an address of its
own. An absolute URL passed as `path` is treated as a path, so no spelling of
the argument can move the request to a different host. The host was chosen
once, by the person who saved the connection.

Pagination stops at the endpoint's last page, at Octa's row cap
(**Settings → Performance**), or at a fixed page ceiling, whichever comes
first. If a `Link` header or a body cursor points at a different host, the
fetch stops rather than following it: those values are data the server chose,
and an endpoint should not be able to walk the read onto an address the user
never approved.
