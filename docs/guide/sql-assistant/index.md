# SQL and assistant

Querying with DuckDB SQL, and asking a language model about your data in plain words.

![SQL and assistant](../../assets/screenshots/sql-view.png)

Each row is one job, with where to find it in the app, on the command line
and as an MCP tool for AI agents. A dash means that surface does not offer it.

| I want to...                         | In the app                                                                  | Command line                                    | MCP tool                                        |
|--------------------------------------|-----------------------------------------------------------------------------|-------------------------------------------------|-------------------------------------------------|
| Query open files with SQL            | [SQL Panel](../../usage/sql.md)                                             | [`--sql`](../../cli/sql.md)                     | [`run_sql`](../../mcp/tools/run_sql.md)         |
| See how a query would run            | [Explain a query](../../usage/sql.md#explain-a-query)                       | [`--explain`](../../cli/sql.md#explain-a-query) | [`run_sql`](../../mcp/tools/run_sql.md#explain) |
| Ask the assistant about your data    | [Chat Assistant](../../usage/chatbot.md)                                    | -                                               | -                                               |
| Set up a model, an API key or Ollama | [Assistant Setup](../../usage/assistant-setup.md)                           | -                                               | -                                               |
| What the assistant sees and sends    | [How the Assistant Understands Your Data](../../usage/assistant-context.md) | -                                               | -                                               |

To let an assistant outside Octa (Claude Desktop, Claude Code, any MCP client) work on your files, see the [MCP server](../../mcp/index.md).
