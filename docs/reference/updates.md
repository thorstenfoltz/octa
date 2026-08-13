# Updates

Octa checks once per launch whether a newer version has been released, and
offers to show you what changed. Both halves are optional and both live under
**Settings > Updates**.

## Check for updates at start

On by default. One request goes to GitHub asking for the latest release. It
reads a version number and nothing else: the check never downloads a binary and
never installs anything on its own.

If GitHub cannot be reached, or you already have the newest version, Octa stays
quiet. A failed check at launch is not worth a pop-up, and neither is "you are
up to date".

Turn the setting off and Octa never contacts GitHub unless you ask it to
through **Help > Check for Updates**, which is unchanged.

## Show what a new release brings

On by default. The first time Octa sees a version you do not have, it opens a
window with that release's notes, taken straight from the release page and
rendered as Markdown.

| Button     | What it does                                                             |
|------------|--------------------------------------------------------------------------|
| Update now | Hands over to the usual update dialog: download, install, restart prompt |
| Close      | Leaves everything as it is; you can update later from the Help menu      |

The window appears **once per version, not once per launch**. Closing it records
the version, so the same notes never interrupt you twice. A release published
without notes still announces itself, just with nothing to read.

The same window also opens for the version you are **already running**, the
first time Octa sees it. So an update brings its own notes with it instead of
you having to wait for the release after it. That form is titled *What's new in
Octa x.y.z*, carries the notes and nothing else: there is no **Update now**
button, because there is nothing to install.

The window also carries a **Do not show this again** tick box. Ticking it is the
same as turning the setting off, and **Settings > Updates** turns it back on.

With release notes switched off but the start-up check left on, an available
version is mentioned once in the status bar instead of opening a window. Turning
the start-up check off silences both.

## What the check sends

A single HTTPS GET to `api.github.com/repos/thorstenfoltz/octa/releases/latest`,
identifying itself with Octa's version in the `User-Agent` header. No account,
no token, and nothing about your files or your machine. See the
[privacy policy](../privacy.md).

## Microsoft Store copies

A copy installed from the Microsoft Store is updated by the Store itself. Octa
cannot replace its own files inside `WindowsApps`, so the update button is left
out of both windows and each names the Store as the thing that will do the
updating. **Help > Check for Updates** is still there and still works; only the
install is missing.

The check and the release notes still work there, so you can read what is coming
before the Store gets round to installing it.

## Installing an update

The in-app updater downloads the release archive for your platform, verifies it
against the release's `SHA256SUMS`, and replaces the binary in place. A mismatch
aborts the update rather than installing anything.

On Linux, if Octa lives somewhere your user cannot write (`/usr/local/bin`, for
example), it stages the new binary first and then asks for a password through
`pkexec`. Nothing is downloaded twice.

Restart Octa once the update finishes; the running process keeps the old code
until you do.
