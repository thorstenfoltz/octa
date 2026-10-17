# Notifications

Octa can tell you when a long task is done, so you can work in another
program meanwhile. It is on by default; **Settings > Window > Notify me
when a long task finishes** turns it off.

## When a notification appears

All three must be true:

1. The setting is on.
2. The task ran for at least **Notify after (seconds)**: 5 by default, any
   whole number of 1 or more. Shorter tasks finish while you are still
   watching.
3. Octa is not the window in front. If you are looking at Octa, there is no
   pop-up, unless **Also when Octa is in front** is ticked (off by default).

The notification says what finished and how long it took, plus the message
Octa showed in its status bar at that moment, which tells you whether it
worked. Every task Octa runs in the background counts:

- SQL queries, local and on a database.
- Reading from a database: a table, Load whole table, table details,
  analyses run on the server, writing back.
- Opening a large file, or a file from a cloud bucket, an API or a web
  address; uploading to a cloud bucket.
- Batch convert, Split into files, Export tables to files, Union, cloud and
  database copies.
- Compare with database or cloud, Compare folders, comparing versions of a
  Delta or Iceberg table, Merge versions, Cell history.
- Multi-search, searching the Databases and Cloud sidebars, a bucket
  inventory.
- Report, Schema drift, Data drift, Harmonise schemas, Fuzzy join, Find
  near-duplicates, Find lookup tables, Relationship map, Clean-up
  suggestions.
- An Assistant reply or an Ask question.

When several run at once, you get one notification when the last one ends.

**Send test notification**, next to the setting, shows one right away.

## Your system

- **KDE Plasma**: notifications appear in Plasma's pop-up and history under
  Octa's name and icon. **System Settings > Notifications > Applications >
  Octa** mutes them or changes how they show. GNOME and other Linux desktops
  use the same notification service.
- **Windows**: nothing to install. The Store version shows its notifications
  as Octa. The zip and installer versions register Octa's name and icon for
  your user account before their first notification (one registry entry, no
  admin rights).
- **macOS**: Notification Centre. macOS asks once whether Octa may send
  notifications.

Nothing is stored and nothing runs in the background for this: Octa notices
the moment it was already noticing that the task is done.

!!! note "Minimised windows on Wayland"
    Some Wayland desktops stop drawing a window that is minimised. Octa then
    notices the end of a task only when its window is shown again, and the
    notification comes late. A window that is merely behind others is not
    affected.
