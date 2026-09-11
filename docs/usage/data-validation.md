# Data Validation

<!-- SCREENSHOT: validation-rules-file.png: The Data validation dialog with three rules listed and the footer row showing "Add rule", "Clear all", "Save rules..." and "Load rules..." beside the live violation count. -->

Data validation flags cells that break a rule you define, painting each
failing cell **red** so problems stand out at a glance. Open it via
**Data -> Data validation...**.

![Data validation](../assets/screenshots/data-validation.png){ .screenshot-placeholder }

## Rules

The dialog holds a list of rules. Each rule has a **column** (a specific
column, or `(any column)` to check every cell) and a **kind**:

| Kind                | A cell fails when...                                                                                                      |
|---------------------|---------------------------------------------------------------------------------------------------------------------------|
| **Not empty**       | the cell is empty or blank.                                                                                               |
| **In range**        | the value is not a number, or falls outside the optional **min** / **max** (leave a bound blank to leave that side open). |
| **Matches pattern** | the text does not match the regular expression.                                                                           |
| **Unique**          | the value is duplicated elsewhere in the column.                                                                          |
| **Max length**      | the text is longer than the given number of characters.                                                                   |

The footer shows a live count of how many cells currently fail.

## How it behaves

Rules apply **live**: failing cells are highlighted the moment you add or
edit a rule, and the highlight updates as you change cell values. The
validation highlight is **per tab and session-only** - it is not saved
with the file and does not change the data, only how it is shown. A manual
[colour mark](../usage/editing.md) or a
[conditional-formatting](conditional-formatting.md) colour takes priority
over the red validation highlight.

**Add rule** appends a new rule, the **X** button removes one, and **Clear
all** removes them all.

## Stepping through violations

Violations are painted in place, which is no help in a table with two
hundred thousand rows. <kbd>F10</kbd> jumps to the next flagged cell and
<kbd>Shift</kbd>+<kbd>F10</kbd> to the previous one; both wrap around and
the status bar reports `Problem 3 of 27` as you go.

The same keys also step through cells flagged by
[Detect Outliers](detect-outliers.md), since both are "cells worth
looking at". Rows hidden by the current filter are skipped, so the
counter always matches what you can actually see.

## Saving rules to a file

Rules live with the tab and disappear when it closes, which is fine
while you are exploring and useless once the same check has to run every
week. **Save rules...** writes the current list to a TOML file:

```toml
[[rule]]
column = "order_id"
kind = "unique"

[[rule]]
column = "amount"
kind = "range"
min = 0
```

**Load rules...** reads one back. Rules are stored by **column name**,
not position, because a rules file outlives the table it was written
from and an index stops meaning anything the moment a column moves.

If a loaded file names a column this table does not have, the dialog
says so and names the columns rather than dropping those rules quietly.
A rules file that half applies is worse than one that fails loudly.

The same file runs from the command line:

```bash
octa --check orders.parquet --rules quality.toml
```

That exits **1** on any violation and on any rule that could not run,
so a CI step can gate on it. See [`--check`](../cli/check.md), and
`check_rules` in the [MCP reference](../mcp/index.md) for the assistant.

## See also

- [Conditional Formatting](conditional-formatting.md) colours cells by a
  rule for emphasis, rather than flagging errors.
- [Search & Filter](search-and-filter.md) can narrow the table to the rows
  you want to inspect before validating.
