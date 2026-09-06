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

Neither the setting nor the menu entry exists in a Microsoft Store copy: see
[Microsoft Store copies](#microsoft-store-copies) below.

## Show what a new release brings

On by default. After an upgrade, Octa opens a window titled *What's new in Octa
x.y.z* with the notes for the version you are now running, rendered as Markdown.

The notes are **built into Octa**. They are the same text the release page
carries, shipped inside the binary, so the window costs no request, works
offline and works on a Microsoft Store copy. It is also independent of the
update check: turning "check for updates at start" off does not silence it.

The window opens **at every start until you dismiss it**:

| Action                                        | What happens next                                        |
|-----------------------------------------------|----------------------------------------------------------|
| **Close**                                     | It opens again the next time you start                   |
| **Do not show these notes again**, then Close | Silent for this version; the next release opens it again |
| Setting turned off here                       | Silent for every version                                 |

So the tick box answers "I have read these", not "never show me release notes".
Only the setting does the latter.

With the start-up check left on, an available new version is mentioned once in
the status bar. Its notes are on the release page, and inside Octa once you have
upgraded.

## What the check sends

A single HTTPS GET to `api.github.com/repos/thorstenfoltz/octa/releases/latest`,
identifying itself with Octa's version in the `User-Agent` header. No account,
no token, and nothing about your files or your machine. See the
[privacy policy](../privacy.md).

## Microsoft Store copies

A copy installed from the Microsoft Store is updated by the Store itself, in the
background, and Octa stays out of it entirely. There is **no update check at
all** in a Store copy:

- **Help > Check for Updates** is not in the menu.
- The launch-time check never runs, so no version is ever announced in the
  status bar.
- **Settings > Updates > Check for updates at start** is greyed out and says why.

Octa cannot replace its own files inside `WindowsApps`, so an install button
there could only fail, and the check that led to it was answering a question
nobody had to ask: Windows already knows about the new version and installs it
on its own.

The release notes are unaffected: they ship inside the copy the Store installed,
so an upgrade announces itself there like anywhere else. Everything under
**Show what a new release brings** works exactly as it does elsewhere.

## Installing an update

The in-app updater downloads the release archive for your platform, verifies it
against the release's `SHA256SUMS`, and replaces the binary in place. A mismatch
aborts the update rather than installing anything, and so does a `SHA256SUMS`
file that cannot be fetched: the update never proceeds unverified. If that
happens, download the release manually from the releases page.

On Linux, if Octa lives somewhere your user cannot write (`/usr/local/bin`, for
example), it stages the new binary first and then asks for a password through
`pkexec`. Nothing is downloaded twice.

Restart Octa once the update finishes; the running process keeps the old code
until you do.
