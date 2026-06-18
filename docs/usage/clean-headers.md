# Clean Headers on Load

**Clean headers on load** is an optional setting (under
**Help > Settings**) that tidies column names the moment a file opens,
turning headers like `First Name` or `E-mail Address` into lower snake_case
identifiers (`first_name`, `e_mail_address`).

## What it does

Each header is trimmed, lowercased, and has spaces and punctuation replaced
with single underscores; leading and trailing underscores are stripped.
Duplicate results get a numeric suffix (`name`, `name_2`) so every column
keeps a distinct name. A header with no usable characters becomes `column`.

It is **off by default**, so files load with their original headers unless
you opt in. It runs after, and pairs naturally with, **Trim whitespace on
load**. The status bar reports how many headers were changed.
