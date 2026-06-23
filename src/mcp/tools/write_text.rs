//! Chat tool: `write_text` - write plain-text content to a file. This is a
//! **chat-only** tool (no MCP `handle`). It writes prose / source code /
//! Markdown either to a new file in the export directory or back to an open
//! tab's file on disk. Writing back to an open tab updates the file, not the
//! live editor: the user must reload (Ctrl+R) to see the change in Octa.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Map, Value};

use super::ToolContext;

pub const DESCRIPTION: &str = "Write plain text (prose, source code, or Markdown) to a file. Give `content` plus either \
`open_tab` (a handle like \"#2\", \"@active\", or the tab name) to overwrite that tab's file on \
disk, or `path` (a bare file name; it is written into the export directory, where all new files \
go). Overwriting an open tab's file does NOT refresh the live editor - tell the user to reload \
(Ctrl+R) to see it. Returns the path written and the byte count.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// The full new text content of the file.
    pub content: String,
    /// Overwrite the file backing this open tab (handle "#2", "@active", or
    /// the tab name). The tab must have been loaded from / saved to a file.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// Destination when not targeting an open tab. A bare name lands in the
    /// export directory; writes are confined there.
    #[serde(default)]
    pub path: PathBuf,
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let dest: PathBuf = if let Some(tabref) = &p.open_tab {
        // Resolve the open tab and write back to its source file. The path is
        // already in `allowed_read_paths`, so `ensure_readable` permits it.
        let snap = if tabref == "@active" {
            ctx.active_tab.and_then(|i| ctx.open_tabs.get(i))
        } else {
            ctx.open_tabs
                .iter()
                .find(|t| &t.handle == tabref || &t.display_name == tabref)
                .or_else(|| ctx.snapshot_for_pathish(tabref))
        }
        .ok_or_else(|| anyhow::anyhow!("no open tab \"{tabref}\""))?;
        let sp = snap.source_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("that tab has no file on disk to write to (it was never saved)")
        })?;
        let path = PathBuf::from(sp);
        ctx.ensure_readable(&path)?;
        path
    } else {
        if p.path.as_os_str().is_empty() {
            anyhow::bail!("provide either `open_tab` or a `path` to write to");
        }
        ctx.resolve_write_path(&p.path)?
    };

    if ctx.backup_before_modify && dest.exists() {
        octa::formats::backup_existing_file(&dest)?;
    }
    std::fs::write(&dest, &p.content)
        .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", dest.display()))?;

    let mut out = Map::new();
    out.insert(
        "path".to_string(),
        Value::String(dest.display().to_string()),
    );
    out.insert("bytes_written".to_string(), Value::from(p.content.len()));
    Ok(Value::Object(out))
}
