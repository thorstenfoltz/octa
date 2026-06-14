# Transform Column

Transform Column reshapes your data with a single click, the way you would
clean up a messy spreadsheet by hand. Open it via
**Edit -> Transform column...**, pick an operation, fill in its options, and
press **Apply**.

![Transform column](../assets/screenshots/transform-column.png){ .screenshot-placeholder }

## Operations

| Operation             | What it does                                                                                                                                                                                                          |
|-----------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Split column**      | Break one column into several, by a **delimiter**, a **regular expression**, or a **fixed width** (every N characters). New columns are named `<source>_1`, `<source>_2`, ...; rows with fewer parts get empty cells. |
| **Merge columns**     | Join two or more columns into one new column with a separator you choose (for example join First and Last name with a space).                                                                                         |
| **Fill down**         | Copy the nearest non-empty value **downwards** into the empty cells below it.                                                                                                                                         |
| **Fill up**           | The same, but **upwards**. Useful for exports that only show a group label on the first row.                                                                                                                          |
| **Extract pattern**   | Pull the first regular-expression match out of each cell into a new column (for example `#(\d+)` to grab an order number). Non-matching cells are left empty.                                                         |
| **Replace in column** | Find and replace within a single column's cells, using Plain, Wildcard, or Regex matching (the same modes as the search bar).                                                                                         |

## How it behaves

- **Split**, **Merge**, and **Extract** create **new columns** (Split and
  Extract insert them next to the source; Merge appends one at the end).
  **Fill** and **Replace** rewrite the chosen column **in place**.
- For the column-creating operations you can set the **new column name** and
  the **insert position** (1-based, like the Insert-column dialog). Leave
  either blank to accept the default shown as the field's hint. For **Split**
  the name acts as a base, so the parts become `name_1`, `name_2`, ... A name
  that already exists gets a `_2`, `_3`, ... suffix so columns never clash.
- Every transform is **undoable** with Ctrl+Z and is applied to the table in
  memory only - nothing is written to disk until you **Save**.
- Transforms are disabled while [read-only mode](../reference/settings.md) is
  on.
- New cells are produced as text; column types are otherwise unchanged.

## Conditional column (if / else-if / else)

**Edit -> Conditional column...** builds a new column whose value depends on
conditions, like a spreadsheet `IF`/`IFS` or a SQL `CASE`. You add an ordered
list of rules:

```
IF   amount  >  100   -> "high"
ELIF amount  >   50   -> "medium"
ELSE                  -> "low"
```

- Each rule tests one column with an operator (equals, contains, greater than,
  is empty, ...) and writes its **output value** when it matches.
- Rules are checked **top to bottom**; the **first** one that matches wins
  (that is the "else if" behaviour). Reorder them with the up/down arrows.
- If no rule matches, the **Else** value is used.
- Outputs that look like numbers become numeric cells (so the new column can be
  summed or sorted as numbers); everything else is text.
- The result is a brand-new column (name and position configurable) and is
  **undoable** with Ctrl+Z. Shortcut: <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>J</kbd>.

This reuses the same comparison operators as
[Conditional formatting](conditional-formatting.md); the difference is that
conditional formatting **colours** matching cells, while a conditional column
**sets a value**.

## See also

- [Formulas](formulas.md) add a computed column from an arithmetic
  expression.
- [Search & Filter](search-and-filter.md) finds and replaces across the whole
  table rather than within one column.
- [Editing](editing.md) covers manual cell edits, undo/redo, and column tools.
