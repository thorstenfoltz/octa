//! `--api NAME`: read a saved REST/JSON endpoint (Settings -> API endpoints)
//! as a table.
//!
//! The endpoint is addressed by the connection's **name**, never by a URL. A
//! human saved the host and its credential; this action only invokes what they
//! saved, which is what makes the same surface safe to hand to the assistant.

use anyhow::Result;

use octa::api::{self, client};
use octa::ui::settings::{AppSettings, api_secrets};

use super::OutputFormat;
use super::output::write_table;

/// Fetch `path` (or the connection's own) and print it.
pub fn run(name: &str, path: Option<&str>, format: OutputFormat) -> Result<()> {
    let settings = AppSettings::load();
    let conn = api::find_connection(&settings.api_connections, name)?;
    let secret = api_secrets::get_api_secret(&conn.id, &settings);

    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let opts = client::FetchOptions {
        path: path.map(|s| s.to_string()),
        // `None` defers to `initial_load_rows()`, which `--rows` already
        // moves through a global guard installed in `dispatch`.
        max_rows: None,
        max_pages: None,
    };
    let out = client::fetch_table(&conn, secret.as_deref(), &opts, &cancel)?;

    // Say so when the read stopped on a limit: a truncated table that looks
    // complete is worse than a slow one.
    if out.capped {
        eprintln!(
            "note: stopped after {} pages / {} rows; pass --rows to raise the limit",
            out.pages,
            out.table.row_count()
        );
    }
    write_table(&out.table, format)
}
