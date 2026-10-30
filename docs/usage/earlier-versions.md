# Earlier Versions

Saving over a file replaces it. With **Keep earlier versions** turned on,
Octa first keeps a copy of the version a save is about to replace, so you can
look at it again, compare it with what you have now, or put it back. Think of
it as the version history a document editor has, for plain data files.

It is **off by default**, because the copies use disk space.

## Turning it on

**Settings > Saving files > Keep earlier versions.** Next to it you set two limits:

| Limit                  | Default | What it means                                                                                |
|------------------------|---------|----------------------------------------------------------------------------------------------|
| Versions per file      | 10      | How many earlier versions to keep of each file. When there are more, the oldest goes first.  |
| Space for all versions | 1024 MB | How much disk space all earlier versions of all files may use together. The oldest go first. |

Each limit has its own **Unlimited** box, so you can use one limit, both, or
neither. With both set to Unlimited, versions are never deleted, and the
settings say so.

The newest version of each file always stays, even when it alone is larger
than the space limit: a limit that deleted the only way back would defeat the
point.

## Using a version

**File > Earlier versions...** lists the versions of the active tab's file,
newest first, each with the time that version was last saved and its size.
Pick one, then:

- **Open** shows it in a new tab. Your file is not touched.
- **Compare** opens the [Compare view](view-modes/compare.md) between this tab
  and the version. Double-clicking a version does the same.
- **Restore** puts the version back as the file and reloads the tab. What the
  file holds at that moment is kept as a version first, so a restore can be
  undone the same way. Restore is not offered while the tab has unsaved
  changes, since reloading would throw them away: save or discard them first.

Versions kept before you turned the switch off stay listed and usable.

## What gets a version

- Every **Save** and **Save as** that writes over an existing file.
- [Auto-save](auto-save.md) keeps a file's version only the first time it saves
  that file after Octa starts. Its timer would otherwise fill the per-file
  limit within minutes and push out the versions you actually want.
- A `.gz` or `.zst` file keeps the compressed file.
- A cloud object keeps nothing here: its storage has versioning of its own.
- A database table has no file, so it keeps nothing either.

If keeping the version fails, for example because the disk is full, the save
stops with a message rather than overwriting the file without a way back.

## Where versions are kept

**Settings > Saving files > Keep them in** decides, once Keep earlier versions is on:

| Choice                       | Where the copies go                                                                             |
|------------------------------|-------------------------------------------------------------------------------------------------|
| Octa's own folder (default)  | A `versions` folder inside Octa's settings folder (on Linux usually `~/.config/octa/versions`). |
| A folder you choose          | One folder you pick with **Choose folder...**, for the versions of every file.                  |
| A subfolder next to the file | A `.octa-versions` folder beside each file, with one folder per file inside it.                 |
| Right beside the file        | In the file's own folder: `sales.20261020-140211-123.csv` next to `sales.csv`.                  |

In Octa's own folder or one you choose, each file has its own subfolder, and
each version keeps the file's name and extension after a time stamp. Next to
the file, the name says which file it belongs to. Either way you can also open
a version from there with any program.

Next to the file, the space limit counts the versions in that folder on their
own, since Octa does not search your disk for the others. Changing the choice
moves nothing: versions kept in the old place stay where they are and are still
listed in **File > Earlier versions...**.

## Delta Lake and Apache Iceberg tables

A Delta or Iceberg table keeps its own history: every write, merge or delete
adds a version, and the old ones stay readable until they are cleaned up. On
a tab opened from such a table folder, **File > Earlier versions** lists that
history instead of kept copies. The **Keep earlier versions** setting plays
no part here.

Each line shows the version (`v12` for Delta, `snapshot 3` for Iceberg), when
it was written, what happened (`WRITE`, `MERGE`, `DELETE`, `append`, ...) and
the rows it added, removed and updated (`+120 -3 ~7`) where the table
recorded them. A table with more than 500 versions shows the newest 500 and
says how many there are.

- **Open** shows the version in a new tab. It is a copy: Refresh does not
  move it to the latest version.
- **Compare with current** shows the rows that differ between this version
  and the table as it is now.
- **What changed** shows what this one change did: the rows it added and
  removed, compared with the version just before it.

Both compares run in DuckDB over the two complete versions, however large the
table, and open their result as a new tab with a `change` column (`added` or
`removed`). The banner gives the exact totals. An updated row appears twice:
once as removed, with the old values, and once as added, with the new ones.
Columns that exist in only one of the two versions are named in the banner
and left out of the comparison. A long comparison shows a spinner and can be
cancelled.

**Restore** is not offered: Octa reads Delta and Iceberg tables but never
writes them.

If a version's data files have been deleted, usually by `VACUUM`, Octa says
so instead of opening it.

### From the command line and MCP

There is no extra flag. `--sql`, the MCP `run_sql` tool and the SQL panel read
any version directly (the file after `--sql` only provides the `data` table;
the scan reads the folder you name):

```sh
octa --sql data.csv -q "SELECT count(*) FROM delta_scan('sales_delta', version = 12)"
octa --sql data.csv -q "SELECT * FROM iceberg_scan('sales_iceberg', allow_moved_paths = true, snapshot_from_id = 3776207205136740581)"
```

## See also

- [Saving](saving.md)
- [Compare view](view-modes/compare.md)
- [Settings](../reference/settings.md#saving-files)
