# Clean and protect

Finding what is wrong with a table, fixing it, and making it safe to share.

![Clean and protect](../../assets/screenshots/cleanup-panel.png)

Each row is one job, with where to find it in the app, on the command line
and as an MCP tool for AI agents. A dash means that surface does not offer it.

| I want to...                              | In the app                                                  | Command line                                  | MCP tool                                                                                                         |
|-------------------------------------------|-------------------------------------------------------------|-----------------------------------------------|------------------------------------------------------------------------------------------------------------------|
| Get clean-up suggestions                  | [Clean-up Suggestions](../../usage/cleanup-suggestions.md)  | -                                             | -                                                                                                                |
| Find and drop duplicate rows              | [Find duplicates](../../usage/editing.md#find-duplicates)   | [`--dedupe`](../../cli/dedupe.md)             | [`find_duplicates`](../../mcp/tools/find_duplicates.md), [`drop_duplicates`](../../mcp/tools/drop_duplicates.md) |
| Find near-duplicates                      | [Find Near-Duplicates](../../usage/find-near-duplicates.md) | -                                             | [`fuzzy_duplicates`](../../mcp/tools/fuzzy_duplicates.md)                                                        |
| Fill missing values                       | [Fill Missing Values](../../usage/fill-missing-values.md)   | [`--impute`](../../cli/impute.md)             | [`fill_missing`](../../mcp/tools/fill_missing.md)                                                                |
| Find outliers                             | [Detect Outliers](../../usage/detect-outliers.md)           | [`--outliers`](../../cli/outliers.md)         | [`detect_outliers`](../../mcp/tools/detect_outliers.md)                                                          |
| Check values against rules                | [Data Validation](../../usage/data-validation.md)           | [`--check`](../../cli/check.md)               | [`check_rules`](../../mcp/tools/check_rules.md)                                                                  |
| Get a data quality report                 | [Data Quality Report](../../usage/data-quality-report.md)   | -                                             | -                                                                                                                |
| Spot values typed in the wrong format     | [Value Shapes](../../usage/value-shapes.md)                 | [`--shapes`](../../cli/shapes.md)             | [`value_shapes`](../../mcp/tools/value_shapes.md)                                                                |
| Find personal data                        | [Detect PII](../../usage/detect-pii.md)                     | [`--detect-pii`](../../cli/pii.md)            | [`detect_pii`](../../mcp/tools/detect_pii.md)                                                                    |
| Anonymise columns                         | [Anonymise Columns](../../usage/anonymize-columns.md)       | [`--anonymize`](../../cli/anonymize.md)       | [`anonymize`](../../mcp/tools/anonymize.md)                                                                      |
| Hash columns                              | [Hash Columns](../../usage/hash-columns.md)                 | [`--hash-columns`](../../cli/hash-columns.md) | [`hash_columns`](../../mcp/tools/hash_columns.md)                                                                |
| Make test data shaped like the real thing | [Test Data](../../usage/test-data.md)                       | [`--test-data`](../../cli/test-data.md)       | [`generate_test_data`](../../mcp/tools/generate_test_data.md)                                                    |
