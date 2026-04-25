## Features

- Add configurable startup window size and maximize-on-startup settings that apply immediately without a restart
- Generate the documentation shortcut table from current bindings and block duplicate shortcut assignments
- Add a Linux update flow that stages binaries in a temp location and prompts for pkexec elevation when the install directory is not writable
- Raise the updater response size limit so large release archives download reliably on Windows

## Fixes

- Tighten table key handling so copy, paste, undo, and redo only consume exact Ctrl-modified input and no longer collide with extended shortcuts
- Remove the cycle-view-mode shortcut to avoid unintended view switches
- Declare per-monitor DPI awareness in the Windows manifest for better scaling on high-DPI and multi-monitor setups

## Refactor

- Split the monolithic `main.rs` into focused `app` modules covering state, rendering, dialogs, file I/O, clipboard, search/replace, edit operations, background row streaming, shortcuts, and updates
- Isolate dialog rendering and central-panel interaction handling to make column edits, confirmations, and update prompts easier to extend
- Update view-mode modules to depend on the new shared app state location without changing behavior

## Chores

- Update `Cargo.lock`
