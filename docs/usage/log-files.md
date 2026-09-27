# Log files

Open a `.log` file (or `syslog`, `messages`, `access_log`, `error_log`, or a
rotated `access.log.1`) and Octa reads it as a table: one row per log entry,
with a real timestamp you can sort and filter, a normalised level, and the
format's own fields as columns. Nothing to configure: Octa looks at the first
500 lines and picks the format that fits most of them.

## Formats

| Format                  | Example line                                                                               |
|-------------------------|--------------------------------------------------------------------------------------------|
| Apache / nginx combined | `203.0.113.9 - - [10/Oct/2026:13:55:36 +0200] "GET / HTTP/1.1" 200 5120 "-" "Mozilla/5.0"` |
| Apache / nginx common   | `203.0.113.9 - - [10/Oct/2026:13:55:36 +0200] "GET / HTTP/1.1" 200 5120`                   |
| syslog (RFC 5424)       | `<165>1 2026-10-11T22:14:15.003Z host app 42 ID47 - message here`                          |
| syslog (RFC 3164)       | `<11>Oct  3 22:14:15 web1 sshd[4721]: Failed password for root`                            |
| JSON lines              | `{"ts":"2026-09-25T10:00:00Z","level":"error","msg":"boom","user":7}`                      |
| logfmt                  | `time=2026-09-25T10:00:00Z level=warn msg="disk almost full" free_mb=512`                  |
| Timestamped text        | `2026-09-25 10:00:01,123 ERROR [main] c.e.Service - failed` (Java, Python logging)         |

## The columns

- **timestamp**: the wall-clock time the line was written. Classic syslog has
  no year, so the file's modification year is used.
- **utc_offset**: the offset the timestamp was written in (`+02:00`), in its
  own column because a date/time cell cannot carry one. Absent when the format
  has none.
- **level**: `WARNING`, `warn` and `W` all become `WARN`; the same for `ERROR`,
  `INFO`, `DEBUG`, `TRACE` and `FATAL` (which also covers `CRITICAL`). Syslog
  levels come from the priority number.
- The format's own fields: `client`, `method`, `path`, `status`, `bytes`,
  `user_agent` for access logs; `host`, `app`, `pid` for syslog; every other
  key for JSON lines and logfmt. Numbers are numbers.
- **message**: the rest of the line.

A column only appears when some row fills it: an access log has no `level`.

## Stack traces and stray lines

A line that continues the entry above it (indented, or starting with `at`,
`Caused by:`, `Traceback` or `...`) is added to that entry's message, so a
Java or Python stack trace stays with its error.

Any other line that fits no entry is kept, never dropped: it gets its own row
with only the **raw** column filled, and a banner above the table says how many
there were.

## When it is not a log

A `.log` of notes (or anything where less than 60% of lines fit one format)
opens as plain text, exactly as before. To use the best format anyway, choose
**View -> Reopen as -> Log**.

Log files load up to the initial row limit (Settings -> Performance) like any
other file; the status bar says when there is more.

See `samples/features/access.log` and `samples/features/app.log` for one of
each.
