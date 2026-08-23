# `--relationships`

Rank the likely relationships between the tables in a folder.

```bash
octa --relationships ./exports
octa --relationships ./exports --recursive
octa --relationships ./exports -f json
```

Prints one row per candidate pairing, best first:

| Column                          | Meaning                                           |
|---------------------------------|---------------------------------------------------|
| `left_table`, `left_column`     | One side of the relationship                      |
| `right_table`, `right_column`   | The other side                                    |
| `score`                         | Overlap weighted by distinctness                  |
| `overlap`                       | Share of the smaller distinct set that matched    |
| `left_orphans`, `left_values`   | Distinct left values with no partner, of how many |
| `right_orphans`, `right_values` | The same counted from the right side              |

Names take no part in the ranking. The score is how much the values
overlap, weighted by how distinct each side is, so a `status` column
that happens to overlap cannot outrank a real key. The orphan counts are
what separate two candidates that score identically, which happens
whenever both tables number their rows from 1. They are printed **both
ways round** because only the count read from the child table's side
settles such a tie, and nothing in the scan knows which side that is.

Always exits **0**: this is a report, not a gate. Files that cannot be
read are named on stderr along with the number of tables scanned.

Reading values is the expensive half, so a scan stops after 30 files and
samples 10,000 rows per table. Add `--recursive` to walk subdirectories.

See [Relationship Map](../usage/relationship-map.md) for the drawn
version, and [Join Key Finder](../usage/join-key-finder.md) for the same
ranking over the tables you have open.
