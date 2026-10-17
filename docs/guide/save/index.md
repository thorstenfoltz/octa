# Save and export

Writing back, converting to another format, and exporting for other people.

![Save and export](../../assets/screenshots/first-steps-file-menu.png)

Each row is one job, with where to find it in the app, on the command line
and as an MCP tool for AI agents. A dash means that surface does not offer it.

| I want to...                        | In the app                                                                                             | Command line                                             | MCP tool                                                                                 |
|-------------------------------------|--------------------------------------------------------------------------------------------------------|----------------------------------------------------------|------------------------------------------------------------------------------------------|
| Save, and save as another format    | [Saving](../../usage/saving.md)                                                                        | [`--convert`](../../cli/convert.md)                      | [`convert`](../../mcp/tools/convert.md), [`write_table`](../../mcp/tools/write_table.md) |
| Export several tabs as one workbook | [Exporting several tabs as one workbook](../../usage/saving.md#exporting-several-tabs-as-one-workbook) | [`--to-workbook`](../../cli/index.md#convert-and-export) | [`write_workbook`](../../mcp/tools/write_workbook.md)                                    |
| Save automatically                  | [Auto-save](../../usage/auto-save.md)                                                                  | -                                                        | -                                                                                        |
| Save a file with a password         | [Password-protected files](../../usage/protected-files.md#saving-with-a-password)                      | `--password-env`                                         | -                                                                                        |
| Export to PDF                       | [Export to PDF](../../usage/pdf-export.md)                                                             | -                                                        | -                                                                                        |
| Export the schema as DDL or code    | [Schema Export](../../usage/schema-export.md)                                                          | [`--export-schema`](../../cli/export-schema.md)          | [`export_schema`](../../mcp/tools/export_schema.md)                                      |
