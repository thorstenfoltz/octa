# Release notes

A batch of display, dialog, SQL, and assistant improvements, plus broader
non-Latin text rendering and finished translations for the five newest languages.

## Data display

**Non-Latin text now renders.** Cell values containing Chinese, Japanese, or
Korean characters previously showed as empty boxes. Octa now bundles a Noto Sans
CJK fallback face, so that text displays correctly in the table even while the
interface itself is in a Latin-script language. (Colour emoji and right-to-left
scripts such as Arabic and Hebrew are still not rendered.)

## Windows and dialogs

**Maximise button now restores the previous size.** Clicking the maximise
button a second time in the Settings, Documentation, Column Inspector, Column
Filter, Value Frequency, and Schema Export windows now shrinks the window back
to the size it had before, instead of staying full-size.

**Number format opens without picking a column first.** **Edit -> Number
format...** now opens straight away (as long as the table has at least one
numeric column); you choose which columns to format from the dialog's "Apply to"
list. Previously it refused to open unless you had selected a numeric column
beforehand.

## SQL

**Copy from the result grid.** Click a cell in the SQL result to select it and
copy it with <kbd>Ctrl</kbd>+<kbd>C</kbd>, just like the main table. Right-click
a result cell for **Copy cell** or **Copy all** (the whole result as TSV).

## Assistant

**`@`-mention autocomplete.** Typing `@` in the chat input now shows a
suggestion popup of your open tabs (by handle and name) and their column names,
so referencing a specific tab or column is quicker. Use <kbd>Tab</kbd> or click
to accept, <kbd>Esc</kbd> to dismiss.

## Documentation and settings

**Searchable in-app documentation.** The built-in Documentation window
(**Help -> Documentation**) now has a search box that filters the section list
as you type.

**"Open as text" moved to Files.** The "Open as text" extension list now lives
under **Settings -> Files** instead of Performance, where it fits better. The
Chat / Assistant settings are now also documented in the online settings
reference.

## Languages

**Finished translations for the five newest languages.** The remaining
interface strings for Indonesian, Vietnamese, Romanian, Hungarian, and Czech are
now translated rather than falling back to English.
