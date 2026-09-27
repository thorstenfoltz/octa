# `list_api_connections`

List the REST/JSON API endpoints saved under **Settings → API endpoints**.

This is the starting point for [`query_api`](query_api.md): that tool takes a
`connection` argument, and this is where the names come from. It reads Octa's
own settings and **contacts no server**, so it is safe to call first and costs
nothing.

Read-only, and kept when the server runs with `--mcp-read-only`.

## Parameters

None.

## Returns

```json
{
  "count": 1,
  "connections": [
    {
      "name": "Orders",
      "base_url": "https://api.example.com/v1",
      "path": "orders",
      "auth": "Bearer",
      "paging": "LinkHeader",
      "records_path": "/data"
    }
  ]
}
```

Credentials are never returned. The `auth` field says only *how* the endpoint
authenticates; the secret itself stays in the OS keyring.

## Notes

An assistant cannot add an endpoint, and cannot call an arbitrary address. The
only endpoints reachable from MCP are the ones a person saved in Settings, and
a `path` is always joined under that endpoint's own base URL. Choosing which
hosts Octa talks to is a decision that stays with the user.
