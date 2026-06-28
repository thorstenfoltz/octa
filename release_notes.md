# Release notes

This release polishes Octa's dialogs. Every pop-up window now shares the same
title bar and window controls, remembers its size, and keeps its action buttons
in easy reach. A handful of remaining English-only labels are now translated in
all 32 languages, and the dependency tree has been refreshed.

## Consistent dialogs

**One look for every window.** All of Octa's dialogs now use the same custom
title bar and minimise, maximise, and close buttons that the main window uses,
instead of a mix of native and custom frames. The change covers the Insert
column, Delete columns, Find duplicates, Value frequency, Column format,
Date/Time calculation, Sheet picker, Table picker, SQL snippet, SQL write-back,
chat prompt, and About dialogs, among others.

**Dialogs remember their size.** Resize a dialog and reopen it later: it comes
back the way you left it, with a predictable starting layout. Transient state is
still reset on close, so each dialog opens clean.

**Action buttons stay put.** Longer dialogs now pin their Apply, Cancel, and
similar buttons to a footer while the content above scrolls, so the controls
never drift off-screen on tall lists or small windows.

## Translations

**No more stray English.** The last few hard-coded strings, spanning dialog
actions, table picking, context menus, and status messages, are now localized
across all 32 supported languages.

## Maintenance

**Refreshed dependencies.** The full dependency tree has been updated to the
latest compatible releases, keeping Octa current on bug fixes and security
patches with no change to behaviour.
