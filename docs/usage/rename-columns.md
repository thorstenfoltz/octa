# Rename Columns

**Columns > Rename columns...** renames many columns at once, instead of editing
each header by hand.

## The list

When you open it, the box is pre-filled with every column of the active tab, one
name per line:

```
id
dob
amount
```

To rename a column, add a comma (or a tab) and the new name to its line. Leave a
line unchanged to keep that column's name:

```
id,user_id
dob,date_of_birth
amount
```

Here `id` and `dob` are renamed and `amount` is left as it is.

As you edit, a live preview shows:

- **Will rename** - lines whose old name was found.
- **Not found** - old names that do not match any current column.
- **Collisions** - a new name that clashes with an existing column or is used
  twice. Apply stays disabled until you resolve them.

**Load from file...** appends more lines from a text file. Applying renames every
matched column as one step, so a single **Undo** reverts the whole batch.
