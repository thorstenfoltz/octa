# Release notes

This release is a big step for working with data, not just viewing it. It adds
a set of data-operations tools (anonymise, find near-duplicates, drop exact
duplicates, fill missing values, detect outliers, detect personal data, union,
join, and partition), an optional header-cleaning step on load, and full
translations of all the new screens into every supported language.

## Cleaning and shaping data

**Anonymise columns.** **Edit -> Anonymise columns...** prepares a file for
sharing by masking or scrambling sensitive columns. Each rule targets one or
more columns with a strategy: a stable **Hash** (the same value always gives the
same code, so the data stays join-able), a **Partial mask** (keep the first or
last few characters), **Redact** (a fixed token or an empty cell), or **Fake** (a
realistic synthetic name, email, city, and so on). An optional salt makes the
output non-guessable, and several columns can be hashed together into one
pseudonymous ID. Output can replace the columns, add new ones, or open a clean
copy in a new tab. Also available on the command line (`octa --anonymize`) and
through the assistant.

**Drop duplicate rows.** **Edit -> Drop duplicate rows...** removes repeated rows
in one undoable step. Tick the columns that make up the key (all of them means
exact whole-row duplicates), and choose whether to keep the first or last of each.

**Fill missing values.** **Edit -> Fill missing values...** fills the empty cells
of a column using the mean, median, mode, a constant, or by carrying the nearest
value forward or backward. Existing values are left untouched.

**Detect outliers.** **Analyse -> Detect outliers...** finds numeric values that
sit far from the rest of their column, using the IQR fence or a Z-score
threshold. You choose what happens when it is done: **highlight** the unusual
cells in orange (a temporary, per-tab view that never changes the data), or add a
real **is_outlier** column you can sort, filter, and save. Number-like text
columns are detected automatically, so columns stored as text are scanned too.

## Finding what is the same (or nearly)

**Find near-duplicates.** **Search -> Find near-duplicates...** groups rows that
are almost the same, catching typos, extra spaces, and reordered words that an
exact match would miss. Pick the columns to compare and a method (token set,
Jaro-Winkler, or edit ratio), set a similarity threshold, and optionally block by
a column so only rows in the same group are compared (faster, and it won't merge
rows that clearly differ). Results highlight the clusters or open in a new tab,
and can add a `cluster_id` column.

**Detect personal data (PII).** **Analyse -> Detect PII...** scans the table for
columns that look like personal data, so you can find sensitive fields before
sharing a file. It weighs two clues for every column: the **column header**
(email, name, gender, country, birthdate, IP, and so on) and the **cell values**
(email, phone, IP, credit card, IBAN, social-security, date, and postal-code
shapes). This is why fields with no give-away values, like names or country, are
still found from their header, while a plain number column like `salary` is left
alone. Each finding shows a plain-language **confidence** and a **basis** (column
name, values, or both), explained right in the dialog. A **Send to Anonymise**
button hands the findings straight to the Anonymise dialog with a hashing rule
per column.

## Combining and splitting tables

**Union tables.** **Analyse -> Union tables...** stacks two or more open tabs on
top of each other into a new table. Octa reconciles the columns: it takes the
union of all columns, fills gaps with empty cells, and widens mismatched number
types to a common type (otherwise text). You can drop columns or override a
column's type before applying.

**Join tables.** **Analyse -> Join tables...** matches rows between two open tabs,
like a spreadsheet VLOOKUP or a SQL JOIN. Pick the left and right tables, then add
one or more conditions pairing **any** column of each side with an operator
(`=`, `<`, `<=`, `>`, `>=`). The column names and types do **not** need to match:
Octa converts both sides to a common type before comparing, so a numeric `id` can
join a text `ref`, and you can match rows where one table's date is `>=` another's.
Choose inner, left, right, or full. Joins run through DuckDB, so they stay fast on
large tables. (The command-line `octa --join` and the assistant join on shared
column names with equality; the dialog is the place for different names or
non-equal operators.)

**Partition by column.** **Analyse -> Partition by column...** splits the active
table into one file per distinct value of a column, written into a folder you
choose, in the format you choose. Partitioning a sales table by `region` gives
you `North.csv`, `South.csv`, and so on. The original table is not changed.

## Opening files

**Clean headers on load.** A new optional setting tidies column names the moment a
file opens, turning headers like `First Name` or `E-mail Address` into lower
snake_case identifiers (`first_name`, `e_mail_address`): trimmed, lowercased, with
spaces and punctuation becoming underscores and duplicates getting a numeric
suffix. It is off by default and pairs naturally with **Trim whitespace on load**.

## Command line and assistant

Every data operation above is also a command-line flag (`--anonymize`,
`--dedupe`, `--impute`, `--outliers`, `--detect-pii`, `--union`, `--join`,
`--partition-by`) and a tool the in-app assistant and MCP server can call, so the
same engines drive the GUI, scripts, and AI workflows.

## Languages

All of the new dialogs, menus, and messages are translated into every supported
language (German, French, Spanish, Italian, Dutch, Portuguese, Polish, Swedish,
Danish, Norwegian, Finnish, Turkish, Indonesian, Vietnamese, Romanian, Hungarian,
Czech, Slovak, Slovenian, Croatian, Serbian, Greek, Russian, Ukrainian,
Bulgarian, Lithuanian, Latvian, Estonian, Japanese, Korean, and Chinese), so the
new tools read in your own language, not English.
