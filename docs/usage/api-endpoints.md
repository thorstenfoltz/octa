# API Endpoints

Octa reads saved REST and JSON endpoints as tables. It applies the endpoint's
authentication and walks its pagination, so opening one gives you every page's
rows rather than the first page.

## Setting one up

Go to **Settings → API endpoints** and fill in the form:

| Field                      | What it is                                                                                     |
|----------------------------|------------------------------------------------------------------------------------------------|
| **Name**                   | What you call it. This is the name `octa --api NAME` and the assistant use.                    |
| **Base URL**               | Scheme, host and any common prefix, e.g. `https://api.example.com/v1`.                         |
| **Default path**           | The path this endpoint reads, e.g. `/orders`.                                                  |
| **Authentication**         | None, bearer token, a key in a header, a key in a query parameter, or a username and password. |
| **Credential**             | The token, key or password. Stored in your OS keyring.                                         |
| **Records path**           | Which array in the response holds the rows. Leave empty to let Octa find it.                   |
| **Pagination**             | How to get past page one. See below.                                                           |
| **Page size**, **Timeout** | Optional tuning.                                                                               |

Press **Test**. It fetches the first page and reports how many rows came back
and what columns they make, without saving anything. It also lists every array
of objects it found in the response, so you can pick the records path instead
of typing it.

## Pagination

| Mode                       | What it does                                                                                                |
|----------------------------|-------------------------------------------------------------------------------------------------------------|
| **One request only**       | Reads a single response.                                                                                    |
| **Page number**            | `?page=1`, `?page=2`, … stopping on the first empty page.                                                   |
| **Offset and limit**       | `?offset=0&limit=100`, `?offset=100&limit=100`, … stopping on a short page.                                 |
| **Cursor in the response** | Reads a cursor from a path in the body and sends it back as a parameter. Stops when it is missing or empty. |
| **Link header**            | Follows `Link: <…>; rel="next"`, the GitHub style. Stops when there is no next link.                        |

Reading also stops at Octa's row cap (**Settings → Performance**) and at a
fixed page ceiling. When it stops on a limit the status bar says so, because a
truncated table that looks complete is worse than a slow one.

## Opening one

**File → Open API endpoint...**, pick the endpoint, optionally give a different
path, and press Open. The result is an ordinary tab: filter it, chart it, run
SQL against it, save it to a file.

<kbd>Ctrl</kbd>+<kbd>R</kbd> re-runs the fetch, the same key that reloads a file
from disk.

## From the command line and the assistant

```bash
octa --api Orders
octa --api Orders --api-path /customers -f csv > customers.csv
```

See [`--api`](../cli/api.md). The assistant reaches the same endpoints through
the [`query_api`](../mcp/tools/query_api.md) and
[`list_api_connections`](../mcp/tools/list_api_connections.md) tools.

## Why a saved endpoint rather than a URL

The host and the credential are chosen once, by you, in Settings. Everything
else invokes what you saved:

- A **path** is always joined *under* the endpoint's base URL. An absolute URL
  passed as a path is treated as a path, so no command-line argument and no
  tool call from the assistant can move the request to a different host.
- **Redirects are not followed.** An endpoint that bounces elsewhere is a URL
  to fix in Settings, not a hop to take silently.
- A **next-page link pointing at another host stops the read** with an error.
  Those links are data the endpoint chose, and an endpoint should not be able
  to walk the fetch onto an address you never approved.

That is what makes these endpoints safe to expose to the assistant: it can use
them, but it cannot invent one.

Unlike the cloud and database connections, an API endpoint has no write path.
It is a read-only source.

## When you just want one address

For a one-off, there is nothing to configure: **File → Open URL...** downloads
an `http(s)://` address and opens it. That path is unauthenticated and makes a
single request by design. API endpoints exist for the cases it cannot reach.
