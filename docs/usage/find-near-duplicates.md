# Find Near-Duplicates

**Search > Find near-duplicates...** (Ctrl+Shift+U) finds rows that are *almost*
the same on the columns you choose, not just exactly equal. It catches typos,
spacing, and reordered words (for example "Jon Smith" vs "John Smith", or
"ACME Inc" vs "ACME, Inc.") and groups the likely duplicates into clusters with
a similarity score for review. It sits beside the exact **Find duplicates**
finder.

## Columns to compare

Tick the columns that should decide whether two rows are alike. For each
candidate pair of rows, Octa scores how similar they are in each of these
columns and averages the scores; the pair is a near-duplicate when that average
is at or above the threshold.

## Method

How the similarity of two text values is measured:

- **Edit ratio** - counts single-character changes (insert / delete / replace).
  Best for **typos** ("color" vs "colour").
- **Jaro-Winkler** - rewards values that start the same way. Best for **names
  and short strings** ("Catherine" vs "Katherine").
- **Token set** - compares the *set of words*, ignoring their order and
  punctuation. Best when **words are reordered** ("Jon Smith" vs "Smith, Jon").

## Similarity threshold

How alike two rows must be to count as near-duplicates, as a percentage. 100%
means identical; lower values catch looser matches but risk false matches. The
default is 85%. Lower it if real duplicates are being missed; raise it if
unrelated rows are being grouped.

## Normalise before comparing

Three clean-up toggles (all on by default) applied before comparing: ignore
case, collapse runs of spaces, and ignore punctuation. These are what let
"ACME, Inc." line up with "ACME Inc".

## Only look for duplicates within the same... (optional grouping)

The dialog has **two** column choices that do different jobs - this is the part
people find confusing, so here it is head to head:

- **Columns to compare** - the columns whose text is matched *loosely*. This is
  where typos and near-misses are found ("Jon" vs "John").
- **Only look for duplicates within the same** - an optional column whose value
  must match *exactly* before two rows are even compared.

Think of the second one as first sorting the table into bins, then hunting for
duplicates **inside each bin only**.

### Worked example

You are de-duplicating a customer list. You set **Columns to compare = name**
and **Only look for duplicates within the same = country**:

| name       | country |
|------------|---------|
| Jon Smith  | US      |
| John Smith | US      |
| Jon Smith  | DE      |

Octa compares the two **US** rows and flags "Jon Smith" is nearly "John Smith".
It never compares the German "Jon Smith" against the US rows, because they fall
in different country bins.

### Why use it

- **Speed** - far fewer comparisons on large tables (otherwise every row is
  compared against every other row).
- **Precision** - it will not merge two rows that happen to share a name but
  clearly differ on a field you trust (country, year, customer type).

Leave it empty to compare every row against every other row.

## Row limit

Caps how many rows are scanned (default 20,000). If the table is larger, the
result says how many rows were actually compared.

## Output

Tick any combination of:

- **Add a cluster_id column to the table** (default) - writes a `cluster_id`
  column (and a `cluster_score` column) onto the table, so you can sort or
  filter by cluster. Non-clustered rows are left blank. This is one undo step.
- **Highlight near-duplicate rows** - colours the rows orange. Re-running first
  clears the previous run's highlight, so it never builds up into a fully marked
  table, and your own manual marks are left alone.
- **Open clusters in a new tab** - a detached report: a `cluster` id and `score`
  column followed by the original columns, grouped by cluster.

The scan runs in the background with a **Cancel** button. Clusters are formed
transitively: if A is near B and B is near C, all three land in one cluster. The
reported cluster score is the lowest linking similarity inside it (the honest
worst case).

The same scan is available as the [`fuzzy_duplicates`](../mcp/tools/fuzzy_duplicates.md)
MCP / assistant tool.
