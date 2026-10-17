# Automate

Doing the same job again without clicking through it: recipes, batches, scripts and agents.

![Automate](../../assets/screenshots/recipe-panel.png)

Each row is one job, with where to find it in the app, on the command line
and as an MCP tool for AI agents. A dash means that surface does not offer it.

| I want to...               | In the app                                    | Command line                                    | MCP tool                                            |
|----------------------------|-----------------------------------------------|-------------------------------------------------|-----------------------------------------------------|
| Replay a recorded clean-up | [Recipes](../../usage/recipes.md)             | [`--recipe`](../../cli/recipe.md)               | [`apply_recipe`](../../mcp/tools/apply_recipe.md)   |
| Convert many files at once | [Batch Convert](../../usage/batch-convert.md) | [`--batch-convert`](../../cli/batch-convert.md) | [`batch_convert`](../../mcp/tools/batch_convert.md) |

Everything in this guide that has a command-line or MCP column can be scripted. Start with the [command-line overview](../../cli/index.md) or the [MCP server](../../mcp/index.md); [shell completions](../../cli/completions.md) and [Docker / Podman](../../cli/docker.md) help with running Octa headless.
