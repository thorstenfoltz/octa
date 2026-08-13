//! CLI cloud actions: browse, transfer and delete objects without the GUI.
//!
//! Everything here is the same code the sidebar and the MCP tools use
//! ([`octa::cloud::ops`]); this module only resolves URLs to providers and
//! renders the result. Credentials come from the saved connections in
//! `settings.toml` when one covers the URL, otherwise from the ambient chain
//! (AWS_* env, cached SSO, Azure CLI login, Google ADC), exactly as the MCP
//! server does.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use octa::cloud::{self, CloudLocation, CloudProvider};
use octa::ui::settings::AppSettings;

use super::OutputFormat;
use super::output::write_table;
use octa::data::{CellValue, ColumnInfo, DataTable};

/// A throwaway table for printing. `DataTable` has no small constructor and
/// every CLI action that prints a computed table builds one this way.
fn table(columns: Vec<(&str, &str)>, rows: Vec<Vec<CellValue>>) -> DataTable {
    DataTable {
        columns: columns
            .into_iter()
            .map(|(name, data_type)| ColumnInfo {
                name: name.to_string(),
                data_type: data_type.to_string(),
            })
            .collect(),
        rows,
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    }
}

/// Recursive-listing cap for `--cloud-ls --recursive`, mirroring the GUI
/// inventory and the MCP `list_objects` tool.
const RECURSIVE_CAP: usize = 100_000;

/// Resolve a cloud URL to a live provider. Thin wrapper over the shared
/// resolver so every surface (these actions, the CLI read path, the GUI
/// compare dialog) picks credentials the same way: a saved connection that
/// covers the URL, else ambient credentials.
fn provider_for(url: &str) -> Result<(Box<dyn CloudProvider>, CloudLocation)> {
    cloud::provider_for_url(url, &AppSettings::load())
}

/// `--cloud-ls URL` - one folder level, or everything under the prefix with
/// `--recursive`.
pub fn ls(url: String, recursive: bool, format: OutputFormat) -> Result<()> {
    let (provider, loc) = provider_for(&url)?;
    let (entries, truncated) = if recursive {
        provider.list_recursive(&loc.key, RECURSIVE_CAP)?
    } else {
        (provider.list(&loc.key)?, false)
    };

    let rows: Vec<Vec<CellValue>> = entries
        .iter()
        .map(|e| {
            vec![
                CellValue::String(if e.is_prefix { "folder" } else { "file" }.into()),
                CellValue::String(e.name.clone()),
                CellValue::String(e.key.clone()),
                e.size
                    .map(|s| CellValue::Int(s as i64))
                    .unwrap_or(CellValue::Null),
                e.modified
                    .map(|m| CellValue::String(m.to_rfc3339()))
                    .unwrap_or(CellValue::Null),
            ]
        })
        .collect();
    write_table(
        &table(
            vec![
                ("type", "Utf8"),
                ("name", "Utf8"),
                ("key", "Utf8"),
                ("size", "Int64"),
                ("modified", "Utf8"),
            ],
            rows,
        ),
        format,
    )?;
    if truncated {
        eprintln!("note: stopped at {RECURSIVE_CAP} objects; the listing is incomplete");
    }
    Ok(())
}

/// `--cloud-get URL --out PATH` - download one object to a local file.
pub fn get(url: String, out: PathBuf) -> Result<()> {
    let (provider, loc) = provider_for(&url)?;
    if cloud::is_prefix(&loc.key) {
        bail!("{url} names a folder; --cloud-get downloads a single object");
    }
    let bytes = provider.get(&loc.key)?;
    std::fs::write(&out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    eprintln!("wrote {} bytes to {}", bytes.len(), out.display());
    Ok(())
}

/// `--cloud-put PATH --to URL` - upload a local file to an object.
pub fn put(local: PathBuf, url: String) -> Result<()> {
    let (provider, loc) = provider_for(&url)?;
    if cloud::is_prefix(&loc.key) {
        bail!("{url} names a folder; --to needs the full destination key");
    }
    let bytes = std::fs::read(&local).with_context(|| format!("reading {}", local.display()))?;
    let len = bytes.len();
    provider.put(&loc.key, bytes)?;
    eprintln!("uploaded {len} bytes to {url}");
    Ok(())
}

/// `--cloud-copy FROM --to TO` and `--cloud-move FROM --to TO`.
pub fn transfer(from: String, to: String, move_it: bool) -> Result<()> {
    let (src, src_loc) = provider_for(&from)?;
    let (dst, dst_loc) = provider_for(&to)?;
    let same_store = src_loc.kind == dst_loc.kind && src_loc.bucket == dst_loc.bucket;
    let op = if move_it {
        cloud::ops::move_
    } else {
        cloud::ops::copy
    };
    let report = op(
        src.as_ref(),
        &src_loc.key,
        dst.as_ref(),
        &dst_loc.key,
        same_store,
    )?;
    let verb = if move_it { "moved" } else { "copied" };
    let how = if report.server_side {
        "server-side"
    } else {
        "streamed"
    };
    eprintln!(
        "{verb} {} object(s), {} bytes ({how})",
        report.objects, report.bytes
    );
    Ok(())
}

/// `--cloud-delete URL` (`--recursive` for a folder).
pub fn delete(url: String, recursive: bool) -> Result<()> {
    let (provider, loc) = provider_for(&url)?;
    if cloud::is_prefix(&loc.key) && !recursive {
        bail!("{url} names a folder; pass --recursive to delete everything under it");
    }
    let report = cloud::ops::delete(provider.as_ref(), &loc.key)?;
    eprintln!("deleted {} object(s)", report.objects);
    Ok(())
}

/// `--list-connections` - the saved cloud and database connections, so a
/// headless run can see what the GUI has configured. Never prints secrets.
pub fn list_connections(format: OutputFormat) -> Result<()> {
    let settings = AppSettings::load();
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    for c in &settings.cloud_connections {
        let target = if c.account_level {
            format!("{}://<account>", c.kind.scheme())
        } else {
            format!(
                "{}://{}/{}",
                c.kind.scheme(),
                c.bucket,
                c.prefix.clone().unwrap_or_default()
            )
        };
        rows.push(vec![
            CellValue::String("cloud".into()),
            CellValue::String(c.name.clone()),
            CellValue::String(c.id.clone()),
            CellValue::String(target),
            CellValue::Bool(c.allow_writes),
        ]);
    }
    for c in &settings.db_connections {
        rows.push(vec![
            CellValue::String("database".into()),
            CellValue::String(c.name.clone()),
            CellValue::String(c.id.clone()),
            CellValue::String(format!(
                "{:?} {}:{}/{}",
                c.engine, c.host, c.port, c.database
            )),
            CellValue::Bool(c.allow_writes),
        ]);
    }
    write_table(
        &table(
            vec![
                ("kind", "Utf8"),
                ("name", "Utf8"),
                ("id", "Utf8"),
                ("target", "Utf8"),
                ("allow_writes", "Boolean"),
            ],
            rows,
        ),
        format,
    )?;
    Ok(())
}
