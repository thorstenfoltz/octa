# Combine and split

Putting tables together, finding how they fit, and taking them apart again.

![Combine and split](../../assets/screenshots/relationship-map.png)

Each row is one job, with where to find it in the app, on the command line
and as an MCP tool for AI agents. A dash means that surface does not offer it.

| I want to...                                            | In the app                                                    | Command line                                                 | MCP tool                                                                                                           |
|---------------------------------------------------------|---------------------------------------------------------------|--------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------|
| Stack tables (union)                                    | [Union Tables](../../usage/union-tables.md)                   | [`--union`](../../cli/union.md)                              | [`union_tables`](../../mcp/tools/union_tables.md)                                                                  |
| Join tables                                             | [Join Tables](../../usage/join-tables.md)                     | [`--join`](../../cli/join.md)                                | [`join_tables`](../../mcp/tools/join_tables.md)                                                                    |
| Join on similar, not equal, values                      | [Fuzzy join](../../usage/fuzzy-join.md)                       | [`--fuzzy-join`](../../cli/fuzzy-join.md)                    | [`fuzzy_join`](../../mcp/tools/fuzzy_join.md)                                                                      |
| Join by location                                        | [Spatial join](../../usage/spatial-join.md)                   | [`--spatial-join`](../../cli/spatial-join.md)                | [`spatial_join`](../../mcp/tools/spatial_join.md)                                                                  |
| Find the columns to join on                             | [Join Key Finder](../../usage/join-key-finder.md)             | [`--unique-columns`](../../cli/unique-columns.md)            | [`suggest_join_keys`](../../mcp/tools/suggest_join_keys.md), [`unique_columns`](../../mcp/tools/unique_columns.md) |
| Find out why a join loses rows                          | [Join Diagnostics](../../usage/join-diagnostics.md)           | -                                                            | [`diagnose_join`](../../mcp/tools/diagnose_join.md)                                                                |
| Find lookup columns                                     | [Find lookup tables](../../usage/lookup-tables.md)            | [`--lookups`](../../cli/lookups.md)                          | [`find_lookups`](../../mcp/tools/find_lookups.md)                                                                  |
| Map how tables relate, in a folder or a database        | [Relationship Map](../../usage/relationship-map.md)           | [`--relationships`](../../cli/relationships.md)              | [`db_relationships`](../../mcp/tools/db_relationships.md)                                                          |
| Check that keys exist in another table                  | [Referential Integrity](../../usage/referential-integrity.md) | [`--check-references`](../../cli/check-references.md)        | [`check_references`](../../mcp/tools/check_references.md)                                                          |
| Bring a folder of files to one schema                   | [Harmonise Schemas](../../usage/harmonise-schemas.md)         | [`--harmonise-schema`](../../cli/index.md#combine-and-split) | [`harmonise_schemas`](../../mcp/tools/harmonise_schemas.md)                                                        |
| Merge copies several people edited                      | [Merge Versions](../../usage/merge-versions.md)               | [`--merge`](../../cli/merge.md)                              | [`merge_tables`](../../mcp/tools/merge_tables.md)                                                                  |
| Split a table into files by a column, row count or size | [Split into Files](../../usage/partition-by-column.md)        | [`--partition-by`](../../cli/partition.md)                   | [`partition_table`](../../mcp/tools/partition_table.md)                                                            |
