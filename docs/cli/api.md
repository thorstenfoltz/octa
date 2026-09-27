# `--api`

Read a saved REST/JSON API endpoint as a table.

```bash
octa --api Orders
octa --api Orders --api-path /customers -f csv > customers.csv
octa --api Orders --rows 500
```

The endpoint is addressed by the **name** of a connection saved under
**Settings → API endpoints**, never by a URL. Its authentication is applied and
its pagination is walked, so what you get is every page's rows.

## Options

| Flag               | Meaning                                                              |
|--------------------|----------------------------------------------------------------------|
| `--api CONNECTION` | Saved connection name or id.                                         |
| `--api-path PATH`  | Path under the endpoint's base URL, overriding the connection's own. |
| `--rows N\|all`    | Raise or lift the row cap for this run.                              |
| `-f FORMAT`        | `table` (default), `csv`, `tsv` or `json`.                           |

## Why a name and not a URL

The host and the credential are chosen once, in the GUI, by a person. This
action invokes what they saved. `--api-path` is always joined *under* that
endpoint's base URL, so it cannot move the request somewhere else, and the
same is true of the matching [`query_api`](../mcp/tools/query_api.md) MCP tool.
That is what makes the endpoint safe to expose to an assistant.

If you want a one-off address instead, there is nothing to configure: point
[`--convert`](convert.md) or any read action at an `https://` URL and Octa
downloads it. That path is unauthenticated and single-request by design.

## When it stops early

Reading stops at the endpoint's last page, at the row cap, or at a fixed page
ceiling. If it stopped on a limit, a note goes to **stderr** naming the page
and row count, so a pipeline reading stdout is unaffected but a person sees it:

```text
note: stopped after 100 pages / 5000000 rows; pass --rows to raise the limit
```

A next-page link pointing at a different host stops the read with an error
rather than being followed.
