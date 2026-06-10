//! Chat tool: `create_chart` - render a chart from tabular data and export it
//! to an image / PDF / SVG file. This is a **chat-only** tool: it lives under
//! `tools/` (so the chat macro picks up its `Params`/`DESCRIPTION`/`run`) but is
//! not registered with the MCP server. It reuses the GUI's pure chart builder
//! (`octa::data::chart`) and exporter (`octa::data::chart_export`).

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Map, Value};

use octa::data::DataTable;
use octa::data::chart::{
    ChartConfig, ChartKind, ChartLimits, LegendPosition, SeriesStyle, build_chart,
};
use octa::data::chart_export::{ExportOptions, to_pdf, to_png, to_svg};

use super::{ToolContext, source_from};

pub const DESCRIPTION: &str = "Render a chart from an open tab (or an open file) and save it as an image (png), pdf, or svg. \
Set `kind` (histogram, bar, line, scatter, box), `x_col` (column name), and `y_cols` (column \
names; bar/line/scatter need at least one, histogram/box use them as the summarised columns, \
histogram needs only x_col). The file is written into the export directory (give a bare file \
name). Returns the output path.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// Open tab to chart: a tab handle like "#2", "@active", or the tab name.
    #[serde(default)]
    pub open_tab: Option<String>,
    /// File to chart instead of a tab (must be a file open in Octa).
    #[serde(default)]
    pub path: PathBuf,
    /// For multi-table sources, the inner table / sheet to chart.
    #[serde(default)]
    pub table: Option<String>,
    /// Chart kind: "histogram", "bar", "line", "scatter", or "box".
    pub kind: String,
    /// X-axis column name. Required for every kind except box.
    #[serde(default)]
    pub x_col: Option<String>,
    /// Y-axis column name(s). Needed for bar / line / scatter; box summarises
    /// these columns; histogram ignores them.
    #[serde(default)]
    pub y_cols: Vec<String>,
    /// Optional chart title.
    #[serde(default)]
    pub title: String,
    /// Output format: "png" (default), "pdf", or "svg".
    #[serde(default)]
    pub format: Option<String>,
    /// Output file name; written into the export directory.
    pub output: PathBuf,
}

fn col_index(table: &DataTable, name: &str) -> anyhow::Result<usize> {
    table
        .columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| anyhow::anyhow!("no column named \"{name}\" in the table"))
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    let table = ctx.resolve(&source_from(&p.open_tab, &p.path, &p.table))?;

    let kind = match p.kind.to_ascii_lowercase().as_str() {
        "histogram" | "hist" => ChartKind::Histogram,
        "bar" => ChartKind::Bar,
        "line" => ChartKind::Line,
        "scatter" => ChartKind::Scatter,
        "box" | "boxplot" => ChartKind::Box,
        other => anyhow::bail!(
            "unknown chart kind \"{other}\" - use histogram, bar, line, scatter, or box"
        ),
    };

    let x_col = match &p.x_col {
        Some(name) => Some(col_index(&table, name)?),
        None => None,
    };
    let y_cols = p
        .y_cols
        .iter()
        .map(|n| col_index(&table, n))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let cfg = ChartConfig {
        kind,
        x_col,
        y_cols,
        title: p.title.clone(),
        ..ChartConfig::default()
    };

    let filtered: Vec<usize> = (0..table.row_count()).collect();
    let prep = build_chart(&table, &filtered, &cfg, ChartLimits::default())
        .map_err(|e| anyhow::anyhow!("could not build chart: {}", e.message()))?;

    let opts = ExportOptions::from_prep(
        &prep,
        cfg.title.clone(),
        "",
        "",
        LegendPosition::default(),
        |_| SeriesStyle {
            display_name: String::new(),
            color: None,
        },
    );
    let svg = to_svg(&prep, &opts);

    let fmt = p.format.as_deref().unwrap_or("png").to_ascii_lowercase();
    let (bytes, ext): (Vec<u8>, &str) = match fmt.as_str() {
        "png" => (to_png(&svg, 2.0).map_err(|e| anyhow::anyhow!(e))?, "png"),
        "pdf" => (to_pdf(&svg).map_err(|e| anyhow::anyhow!(e))?, "pdf"),
        "svg" => (svg.into_bytes(), "svg"),
        other => anyhow::bail!("unknown format \"{other}\" - use png, pdf, or svg"),
    };

    // Force the chosen extension, then confine to the export dir.
    let mut requested = p.output.clone();
    requested.set_extension(ext);
    let out = ctx.resolve_write_path(&requested)?;
    std::fs::write(&out, bytes)
        .map_err(|e| anyhow::anyhow!("could not write {}: {e}", out.display()))?;

    let mut m = Map::new();
    m.insert(
        "output".to_string(),
        Value::String(out.display().to_string()),
    );
    m.insert("format".to_string(), Value::String(ext.to_string()));
    Ok(Value::Object(m))
}
