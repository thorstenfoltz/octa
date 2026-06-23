# Debug Mode and Reports

Octa keeps a small, self-maintaining log so that when something goes wrong
there is a record to look at. Logging is always on, capped in size, and safe
to share once redacted. This page explains exactly what is written, where, and
how the size limit works.

## Where the files live

All diagnostic files live in a `logs` subfolder of Octa's configuration
directory:

| Platform | Folder                                     |
|----------|--------------------------------------------|
| Linux    | `~/.config/octa/logs/`                     |
| macOS    | `~/Library/Application Support/Octa/logs/` |
| Windows  | `%APPDATA%\Octa\logs\`                     |

The fastest way to get there is **Settings -> Diagnostics -> Open log folder**,
which opens this folder in your file manager (creating it first if it does not
exist yet).

The folder can hold:

- `octa.log` -- the current (live) log.
- `octa.log.1` -- the previous log, kept after a rotation (see below).
- `last_crash.txt` -- details of the most recent crash, if any.
- `running.lock` -- a tiny marker used to detect hard crashes (see
  [After a crash](#after-a-crash)).
- `octa-debug-<timestamp>.txt` -- any debug reports you have exported.

## What gets logged, and when

Logging is **enabled by default** -- there is no switch to turn the log on.
Every GUI session writes to `octa.log` from start-up onward. Two levels of
detail are in play:

- **Octa's own code** logs at `info` level by default: notable events such as
  files opened, saves, background loading, and any warnings or errors.
- **Third-party libraries** are kept to `warn` and `error` only, so the log
  stays readable and does not fill up with noise from dependencies.

Setting the `RUST_LOG` environment variable overrides these defaults at
start-up if you need fine-grained control (standard `tracing` filter syntax).

## The size limit and rotation

The log can never grow without bound. The live `octa.log` is capped at about
**5 MB**. Here is exactly what happens when that cap is reached:

1. A log write pushes `octa.log` to (or past) 5 MB.
2. Octa flushes it and renames it to `octa.log.1`, **overwriting** any previous
   `octa.log.1`.
3. A fresh, empty `octa.log` is opened and logging continues there.

So at most **two files** exist at once and the total on disk stays around
**10 MB**. The trade-off is that the oldest entries are eventually discarded:
once a second rotation happens, the very first log is gone. This is deliberate
-- recent history is what matters for diagnosing a problem, and the cap keeps
the folder small.

The same check runs at start-up: if `octa.log` is already at or over the cap
when Octa launches (for example after an unclean shutdown), it is rotated
immediately so a restart cannot keep appending past the bound.

## Debug logging

The only thing that is **off by default** is *debug-level verbosity*. Enable
**Settings -> Diagnostics -> Debug logging** to raise Octa's own code from
`info` to `debug` -- much more detailed entries, useful when reproducing a
specific bug. It takes effect immediately, with no restart, and dependency logs
stay at `warn`/`error` regardless.

Leaving it off is recommended for normal use: debug-level entries fill the
5 MB cap far faster, so the log rotates sooner and less history is retained.
Turn it on only while reproducing an issue, then turn it back off.

## After a crash

Octa records failures two complementary ways, both in the `logs` folder:

- A **panic handler** catches Rust panics, writes the time, location, message,
  and a backtrace to `last_crash.txt`, and also logs the panic to `octa.log`.
- A **run-lock sentinel** (`running.lock`) catches harder crashes the panic
  handler cannot, such as a native segfault or the process being killed. The
  marker is created at start-up and deleted on a clean exit. If it is still
  present at the next launch, the previous run ended uncleanly.

In either case, the next launch offers to export a debug report. You can also
read the raw details directly in `logs/last_crash.txt` and `logs/octa.log`.

## Exporting a report

Use **Help -> Export debug report...** at any time. Octa writes a single text
file, `octa-debug-<timestamp>.txt`, into the `logs` folder and reveals it in
your file manager. The report contains your Octa version, operating system,
theme and language, the tail of the log (about the last 256 KB), the last crash
if there is one, and your settings.

It is **safe to share**: API keys are stripped, and your home directory and
username are masked. No cell values or column data from your files are ever
included.
