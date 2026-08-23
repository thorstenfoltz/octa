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

## Duplicate column names

A file can name two columns the same thing, and then the list above cannot
help: it finds a column by its name, and a repeated name picks out the wrong
one. **Fix duplicate names** in the same dialog handles those by position
instead.

Tick it and the preview lists what it would do. The first column keeps the
name; every later one gets a number: `id`, `id_2`, `id_3`. A suffix another
column already owns is skipped, so `a`, `a`, `a_2` renames only the middle one
(to `a_3`) and leaves the real `a_2` alone.

**Ignore upper/lower case** widens it, so `Name` and `name` count as the same
name and the second one gets numbered. Off by default, since some files mean
those as two different columns.

**Columns > Fix duplicate names...** is the same dialog opened straight onto
this half of it. Either way the renames join the same single undo step.

Duplicate names are worth fixing before you run SQL over the table, join it or
export it: those all address a column by name.
