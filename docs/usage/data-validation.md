# Data Validation

Data validation flags cells that break a rule you define, painting each
failing cell **red** so problems stand out at a glance. Open it via
**Edit -> Data validation...**.

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

## See also

- [Conditional Formatting](conditional-formatting.md) colours cells by a
  rule for emphasis, rather than flagging errors.
- [Search & Filter](search-and-filter.md) can narrow the table to the rows
  you want to inspect before validating.
