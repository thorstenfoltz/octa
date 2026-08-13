# Harmonise Schemas

The write half of [Schema drift](schema-drift.md). That scan tells you 497
parts look like this and 3 look like that; this rewrites the odd ones out.

Open it from **File -> Harmonise schemas...**, or press **Harmonise...** in
the Schema drift dialog, which carries the folder and scan options across.

## Two steps

**Plan** scans the folder and shows what would happen without writing
anything:

- how many files will change, already match, or are refused
- the target columns
- **which columns will be dropped**

**Harmonise** then writes.

The split is deliberate. Dropping a column is the only lossy part of this
operation, so you see it before committing rather than discovering it in the
report afterwards.

## What happens to each file

| Situation                    | Result                                        |
|------------------------------|-----------------------------------------------|
| Column missing from the file | Added, filled with nulls                      |
| Column not in the target     | **Dropped**, and named in the plan and report |
| Column type differs          | Cast to the target type                       |
| Column order differs         | Reordered to match the target                 |

The target schema is the shape **most files already have**, since that needs
the fewest rewrites. Use `--target-file` (CLI) or pick a file explicitly to
override that.

## Two safety properties

### Your files are never modified

Harmonised copies go to a **separate output folder**, which is required
rather than defaulted to something convenient. If the result is not what you
wanted, you have lost some disk space and nothing else.

Two input files from different subfolders that share a file name would write
to the same output. **Both are refused**, rather than one being quietly
renamed: silently renaming part of a dataset hides exactly the ambiguity this
operation must not paper over.

### A file that will not cast is refused, not emptied

If a column holds `not-a-number` and the target wants a whole number, that
file is **skipped with a reason** instead of written with blanks where the
values used to be.

This matters more than it sounds. A harmonised folder full of silently
emptied cells looks perfectly clean, passes every schema check, and has lost
data. Refusing is the only honest answer.

## Command line

```bash
octa --harmonise-schema ./parts --out-dir ./parts-clean
```

Options: `--recursive`, `--ignore-case`, `--overwrite`, and `--target-file
FILE` to name the target schema instead of taking the majority.

**Exits 1 when any file was refused**, like `--schema-drift` and
`--validate-schema`, so a CI step can gate on it. The report goes to stdout;
refusals and dropped columns go to stderr, so a pipe stays parseable.

## Assistant and MCP

The `harmonise_schemas` tool does the same thing. It is a **write tool**, so
it is hidden from chat profiles without "Allow writes" and removed entirely
under `--mcp-read-only`.

## Limits

- Local paths only.
- The first table of a multi-table source, matching the drift scan.
- Schema level only: no row-level repair beyond the cast.
- Files are processed one at a time.

## See also

- [Schema drift](schema-drift.md) - finding the disagreement in the first place
- [Batch convert](batch-convert.md) - changing format rather than schema
- [Transform column](transform-column.md) - reshaping one open table
