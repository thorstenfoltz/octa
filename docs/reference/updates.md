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
through **Help > Check for updates**, which is unchanged.

A Microsoft Store copy has the setting but not the menu entry: see
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

A single HTTPS GET to `github.com/thorstenfoltz/octa/releases/latest`,
identifying itself with Octa's version in the `User-Agent` header. GitHub
answers with the address of the newest release, and Octa reads the version
from that address without following it. No account, no token, and nothing
about your files or your machine. See the [privacy policy](../privacy.md).

## Microsoft Store copies

A copy installed from the Microsoft Store is updated by the Store itself. Octa
cannot replace its own files inside `WindowsApps`, so it never offers to
install anything there:

- **Help > Check for updates** is not in the menu. **Help > Updates in the
  Microsoft Store...** takes its place: it opens Octa's page in the Store app,
  where an **Update** button waits once the Store offers the new version. Octa
  sends no request for this; it only opens the page.
- **Settings > Updates > Check for updates at start** works as it does
  everywhere else, on by default. When it finds a newer version, the status bar
  says so and adds that the Store can take a few days to offer it: if the
  Store page does not show the update yet, come back later.

The check stays useful there because the Store installs updates on its own
schedule, not the moment a release appears, and only when **App updates** is
switched on in the Store's settings. Windows Update does not update Store apps
at all.

The release notes are unaffected: they ship inside the copy the Store installed,
so an upgrade announces itself there like anywhere else. Everything under
**Show what a new release brings** works exactly as it does elsewhere.
