# Mail, calendar and contact files

Octa opens saved mail, mailboxes, calendars and address books as tables. Every
file becomes one table per kind of item it holds, and only the kinds it
actually holds are offered.

| Format                 | Extensions     | What it is                                                                              |
|------------------------|----------------|-----------------------------------------------------------------------------------------|
| Email                  | `.eml`         | One saved mail, the way most mail programs save one                                     |
| Mailbox                | `.mbox`        | A whole mailbox in one file (Thunderbird, Apple Mail, Gmail export)                     |
| Outlook item           | `.msg`, `.oft` | One item saved from Outlook: a mail, appointment, contact or task; `.oft` is a template |
| Outlook data file      | `.pst`         | A whole Outlook mailbox: mail, calendar, contacts, tasks, notes                         |
| Outlook for Mac export | `.olm`         | What Outlook for Mac writes when you export a mailbox                                   |
| iCalendar              | `.ics`, `.vcs` | Calendar exports and invitations (`.vcs` is the older vCalendar)                        |
| vCard                  | `.vcf`         | Address book exports, one or many contacts                                              |

All of them are read only.

## The tables

| Table       | Columns                                                                                                                  |
|-------------|--------------------------------------------------------------------------------------------------------------------------|
| Mail        | `mail_id`, `folder`, `sent`, `received`, `from`, `to`, `cc`, `subject`, `attachments`, `body`, `message_id`              |
| Calendar    | `folder`, `title`, `start`, `end`, `all_day`, `location`, `organiser`, `attendees`, `recurrence`, `description`          |
| Contacts    | `folder`, `name`, `given_name`, `family_name`, `company`, `job_title`, `emails`, `phones`, `address`, `birthday`, `note` |
| Tasks       | `folder`, `subject`, `start`, `due`, `status`, `percent_done`, `priority`, `body`                                        |
| Notes       | `folder`, `text`, `created`, `modified`                                                                                  |
| Attachments | `mail_id`, `file_name`, `size`, `type`, `content`                                                                        |

- **`folder`** is the folder path inside the file, such as `Inbox/Projects`.
  Only mailboxes with folders (`.pst`, `.olm`) have the column.
- **Several values in one cell** (recipients, email addresses, phone numbers)
  are joined with `;`.
- **`mail_id`** is the mail's row number in the Mail table. The Attachments
  table carries the same number, so you can join the two, or filter the
  attachments of one mail.
- **`attachments`** in the Mail table counts the mail's attachments.

## Attachments

Each attachment is one row, and its `content` cell holds the file itself.
Right-click the cell and choose **View contents...** to preview a picture, a
PDF or text, save the file, or open it in the program your system uses for it.
An email attached inside a mail is listed by name with an empty `content`.

A mailbox's attachments are read only when you open the Attachments table, so
opening the Mail table of a large mailbox stays quick.

## What the cells hold

- **Bodies are plain text.** A mail that has only an HTML body is turned into
  text. An old Outlook mail whose body exists only as compressed RTF has an
  empty `body`.
- **Times are shown as written.** A calendar time with a time zone is shown in
  that zone's local time (Octa's timestamps carry no zone), a UTC time stays
  UTC, and a mail's `sent` is the sender's clock as the header says. An
  all-day event has `all_day` set and starts at midnight.
- **Recurring events are one row.** The `recurrence` cell holds the rule as
  text (`FREQ=WEEKLY;BYDAY=TU`); the repeats are not expanded into rows.
- **Text in old encodings is decoded.** Encoded mail headers, quoted-printable
  vCards and Outlook items written in a Windows code page all read as normal
  text.

## Opening

- **One item** (`.eml`, `.msg`, `.oft`) opens all its tables at once: the item
  and, when it has any, its attachments.
- **A mailbox** (`.mbox`, `.pst`, `.olm`) opens the table picker with the
  number of items of each kind, like a database. For a `.pst` the counts come
  from the folder listings, so no mail is opened just to count.
- **A calendar or address book** (`.ics`, `.vcs`, `.vcf`) opens directly. A
  calendar holding both events and to-dos opens both tables.
- A large mailbox reads as many items as the initial rows setting allows.

## Not supported

- **`.ost` and `.nst`**, Outlook's offline caches. Outlook 2013 and later
  write them in a layout no reader outside Outlook can open, and their mail
  lives on the server anyway. Export a `.pst` from Outlook instead
  (**File > Open & Export > Import/Export**).
- **`.pab` and `.nk2`**, Outlook's old personal address book and its
  autocomplete list. Both are long retired and undocumented.

## Known limits

- **ANSI `.pst` files** (Outlook 97 to 2002) are read by the same code as the
  Unicode ones Outlook has written since 2003, but have not been tested
  against a real file.
- **`.olm`** is not documented by Microsoft. Octa follows public descriptions
  of what Outlook for Mac 2011 to 2019 writes, and skips anything it does not
  recognise rather than failing.

## Command line and MCP

The CLI and MCP read these files like any other format. On the command line
an action reads the file's first table, so `octa --head inbox.eml` prints the
Mail table. Through MCP, `list_tables` lists the tables of a `.pst` with their
row counts and `read_table` with a `table` name reads any one of them.

## See also

- [Supported formats](../getting-started/supported-formats.md)
- [Binary cells](binary-cells.md)
