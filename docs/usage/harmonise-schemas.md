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

## Combining into one table

Harmonising gives you a folder of files that finally agree. Often what you
actually wanted was **one** table.

Tick **Combine into one table** in the dialog (or pass `--combine` on the
command line, or `"combine": true` to the tool) and the same inputs are
folded into a single table instead, with a **`source_file`** column saying
which file each row came from. Without that column a combined folder loses
the one thing that made the parts separate.

Combining skips the plan step, because there is nothing to refuse: nothing
is cast, so no value can be lost to a narrowing conversion. Columns that
only some files have are kept and left empty for the files that lacked
them, exactly as [Union tables](union-tables.md) does, because it is the
same engine underneath.

If your data already has a column called `source_file`, it is **not**
overwritten. The provenance column is suffixed (`source_file_2`) instead,
and the suffix is decided once across every input so all the rows land in
the same column.

A file that cannot be read is named, skipped and counted; one corrupt part
does not cost you the other four hundred.

In the GUI the result opens as a new tab, so you can look at it before
deciding where it belongs. On the command line and through the tool it is
written to the path you name.

## Command line

```bash
octa --harmonise-schema ./parts --out-dir ./parts-clean
```

To combine instead, name a single output **file**:

```bash
octa --harmonise-schema ./parts --combine --out ./all-parts.csv
```

Options: `--recursive`, `--ignore-case`, `--overwrite`, and `--target-file
FILE` to name the target schema instead of taking the majority. `--combine`
takes `--out FILE` rather than `--out-dir DIR`, since it produces one file;
asking for it without `--out` is refused rather than guessed at.

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
