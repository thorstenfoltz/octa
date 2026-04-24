//! Binary-only application shell: the `OctaApp` struct and everything that
//! drives it (dialogs, toolbar actions, keyboard shortcuts, view-mode
//! rendering, update-install flow). Lives under `src/app/` so it stays out
//! of the public library surface.

pub(crate) mod bg_rows;
pub(crate) mod central_panel;
pub(crate) mod clipboard;
pub(crate) mod dialogs;
pub(crate) mod edit_ops;
pub(crate) mod file_io;
pub(crate) mod find_replace;
pub(crate) mod init;
pub(crate) mod shortcuts_dispatch;
pub(crate) mod sidebar;
pub(crate) mod sql_panel;
pub(crate) mod state;
pub(crate) mod status_bar;
pub(crate) mod tabs;
pub(crate) mod toolbar_handler;
pub(crate) mod update_check;
pub(crate) mod update_loop;

pub(crate) use state::OctaApp;
