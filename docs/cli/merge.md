# `--merge`

Merge two or more versions of a table, per row and per cell, optionally
against the original they were all edited from. **Exits 1 and writes nothing
while a conflict is open**, which is exactly what git expects from a merge
driver.

```sh
octa --merge anna.csv ben.csv carla.csv --merge-original march.csv \
     --merge-key id --merge-out merged.csv
```

With `--merge-original`, a change made in one version is taken over and only
different changes to the same cell conflict. Without it, every cell where the
versions differ conflicts, and rows from any version are kept.

## Flags

| Flag               | Required? | Description                                                                                                |
|--------------------|-----------|------------------------------------------------------------------------------------------------------------|
| `--merge`          | yes       | The versions, two or more (or one plus `--merge-original`).                                                |
| `--merge-original` | no        | The table every version was edited from.                                                                   |
| `--merge-key`      | no        | Key column(s) that identify a row, comma-separated or repeated. Without one, rows are matched by position. |
| `--merge-prefer`   | no        | Settle every conflict in favour of version N (1 = the first file given to `--merge`).                      |
| `--merge-out`      | no        | Write the merged table to this file. Without it the merge goes to stdout in the `-f` format.               |
| `--merge-format`   | no        | Format of all the files, for files without an extension. An extension (`csv`) or a file name (git's `%P`). |

## Output

A clean merge prints the merged table, or writes it to `--merge-out` and
reports the size on stderr:

```text
merged 3 rows x 3 columns into merged.csv
```

With open conflicts nothing is written. The conflicts go to stdout, one per
row, with one column per version:

```text
row  column  original  version_1  version_2
2    name    b         MINE       THEIRS
```

A row deleted in some versions and edited in another shows `(deleted)` and
`(edited)` in the version columns and no column name.

## As a git merge driver

```text
# .gitattributes
*.csv merge=octa
```

```sh
git config merge.octa.driver "octa --merge %A %B --merge-original %O --merge-key id --merge-out %A --merge-format %P"
```

A merge where the branches touched different rows or cells then completes by
itself. On a real conflict the driver exits 1, git leaves the file marked
conflicted, and the GUI's **File -> Merge versions...** reads the versions
from git. See [Merge Versions](../usage/merge-versions.md).

## Exit codes

| Code | Meaning                                                               |
|------|-----------------------------------------------------------------------|
| `0`  | Merged, and written to `--merge-out` or stdout.                       |
| `1`  | Conflicts are open, a key column is missing, or a file is unreadable. |
