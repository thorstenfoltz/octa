# Detect PII

**Analyse > Detect PII...** scans the table for columns that look like
personal data, so you can find sensitive fields before sharing a file.

## How it works

Octa weighs two clues for every column:

- the **column header** (does it look like `email`, `first_name`, `gender`,
  `country`, `birthdate`, `ip`, ...?), and
- the **cell values** (how many match a known shape: email, phone, IP
  address, credit card, IBAN, SSN, date, postal code).

That is why fields with no give-away values, like names, gender or country,
are still found from their header, while a plain number column like `salary`
is left alone. It only reads the data; nothing is changed.

## Confidence

The percentage combines the two clues:

- a strong value pattern on its own reaches at least 60%;
- a matching header on its own reaches 60%;
- the two together score highest (up to 100%).

A column is listed when its best guess is at least 50%. The **Basis** column
shows which clue drove it: `column name`, `values (N%)`, or both.

## Send to Anonymise

**Send to Anonymise** opens the [Anonymise](anonymize-columns.md) dialog
pre-filled with one hashing rule per detected column, so you can mask the
sensitive fields in a couple of clicks.

## Command line and assistant

Also available as `octa --detect-pii` (see the [`--detect-pii`](../cli/pii.md)
reference) and the [`detect_pii`](../mcp/tools/detect_pii.md) MCP / assistant
tool, which return the same `confidence`, `by_name` and `value_match` fields.
