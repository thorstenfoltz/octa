This release repairs Octa on Linux. On some desktops a large part of the
window quietly ignored every click, the AppImage would not start at all, and
the install script failed halfway through instead of saying what it needed.

**If you are on Windows or macOS, none of those faults were seen on your
platform**, and the AppImage and the install script do not exist there at all.
Two of the changes below do reach every platform, and both are improvements
rather than repairs: Octa now writes its settings file on the first launch, and
debug logging can be switched on from outside the program.

## Buttons that did nothing

On some Linux desktops, Linux Mint's Cinnamon among them, controls in the
lower part of the window did not respond. Apply and Cancel in Settings, the
Close button on this very notes window, buttons at the bottom of other
dialogs: no highlight when the mouse passed over them, and nothing at all when
clicked. The same dialogs could still be dragged around and scrolled, which
made it look as though dialogs in general were broken.

The cause was Octa's own title bar. Because Octa replaces the one your desktop
would draw, it also has to supply the invisible strips along the window edges
that you grab to resize a window. Those strips are meant to be about eight
pixels wide and they sit above everything else on screen, so that a window
edge can always be grabbed. On the affected desktops the bottom strip came out
several hundred pixels tall instead, covering the lower part of the window.
Every click in that band went to the invisible strip rather than to the button
underneath it.

The strips are now positioned exactly, so no desktop can stretch them.

Whether you saw this depended on your desktop rather than your hardware. Octa
starts maximised and skips the strips entirely for a maximised window, since
the desktop handles resizing then. KDE and GNOME report a window as maximised
and so were never affected. Cinnamon does not report it, so the strips were
drawn anyway, and the stretched one landed on top of the dialogs.

Octa draws its own window controls by default, which is what puts it in charge
of moving and resizing the window in the first place. If either misbehaves on a
desktop we have not seen, turn **Window controls in toolbar** off under
**Settings > Appearance**: your desktop then draws its usual title bar and
handles the window itself.

## The AppImage starts

Double-clicking the AppImage could do nothing whatsoever: no window, no error,
no dialog. There were two independent reasons, and both are addressed.

Browsers save a downloaded file without permission to run it, and most file
managers respond to a double-click on an AppImage in that state by doing
nothing at all, silently. The documentation now says so and gives the command
that fixes it. If an AppImage ever seems to be ignored, run it from a terminal
instead: that is where the reason gets printed.

The AppImage also needed the `libfuse2` package, which Ubuntu 22.10 and later,
and Mint 22, no longer install. It now carries that machinery inside itself and
needs nothing installed alongside it.

## The install script says what it needs

`./install.sh` installs into `/usr/local` (`/usr` on Arch Linux), which needs
root. Run without it, the script used to copy the program and then stop on a
permission error at the next step, leaving a half-installed system behind. It
now checks before it copies anything and names the two ways that work:
`sudo ./install.sh` for everyone on the machine, or `./install.sh ~/.local` for
just you. `uninstall.sh` had the same flaw and got the same check.

## A settings file from the very first launch

Octa wrote `settings.toml` only when you pressed Apply in Settings, so a fresh
installation had no settings file at all. That matters when the interface
itself is what is misbehaving, because editing the file by hand is then the
only way to change anything. The file is now written with its defaults the
first time Octa starts.

## Diagnosing the interface itself

Starting Octa as `OCTA_DEBUG=1 octa` turns debug logging on for that one run
without touching your saved settings, and without needing the Settings dialog,
which is precisely what you cannot reach when the interface is the problem. It
records every mouse press and release: where it was, whether it counted as a
click, and which layer of the interface it reached. A build from source also
outlines every clickable area on screen, which shows at a glance whether a
control that looks clickable actually is one.

This is how the fault above was identified after reading the code had failed
to explain it.

## Smaller things

- **Documentation** for all of the above, in the in-app help and on the
  documentation site: what to do when an AppImage seems to be ignored, when
  the install script stops on a permission error, and when a window cannot be
  moved or resized.
- **The manual page lists its environment variables on the documentation
  site.** `man octa` has always described `OCTA_CONFIG_DIR` and its
  companions, but that section was missing from the copy published on the
  site.
