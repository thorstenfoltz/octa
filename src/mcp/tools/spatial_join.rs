//! MCP tool: `spatial_join` - join by location. Read-only; the same join as
//! the GUI's Join dialog **Spatial** type and `octa --spatial-join`.

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Deserialize;
use serde_json::{Value, json};

use octa::data::spatial_join::{Layer, SpatialOp, point_cols, prefix_for, spatial_join};

use crate::mcp::OctaMcpServer;

use super::union::SourceParam;
use super::{ToolContext, source_from, table_to_json};

pub const DESCRIPTION: &str = "Join by location. `points` is a table with latitude/longitude \
columns or point geometry; `layers` are one or more tables. `op` `inside` (default): each point \
gets the columns of the layer polygon it lies in (GeoJSON or shapefile). `op` \
`nearest`: each point gets the columns of the closest layer point and `<layer>_distance_km` \
(great-circle); `within_km` leaves farther points empty. Layer columns are prefixed with the \
layer's file name. Coordinates must be latitude/longitude. Returns `{multi_match, no_point, table}`.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Params {
    /// The table with the points: a `path` or an `open_tab`.
    pub points: SourceParam,
    /// One or more tables to join against, each a `path` or an `open_tab`.
    pub layers: Vec<SourceParam>,
    /// `inside` (default) or `nearest`.
    #[serde(default)]
    pub op: Option<String>,
    /// For `nearest`: points farther than this many km stay empty.
    #[serde(default)]
    pub within_km: Option<f64>,
    /// Maximum rows to return. Pass 0 for unlimited.
    #[serde(default)]
    pub limit: Option<usize>,
}

fn resolve(ctx: &ToolContext, s: &SourceParam) -> anyhow::Result<octa::data::DataTable> {
    ctx.resolve(&source_from(&s.open_tab, &s.path, &s.table))
}

/// A layer's column prefix: the open tab's name, else the file name.
fn layer_name(s: &SourceParam) -> String {
    match &s.open_tab {
        Some(tab) => prefix_for(tab),
        None => prefix_for(&s.path.file_name().unwrap_or_default().to_string_lossy()),
    }
}

pub fn run(ctx: &ToolContext, p: &Params) -> anyhow::Result<Value> {
    anyhow::ensure!(!p.layers.is_empty(), "`layers` needs at least one table");
    let op = match p.op.as_deref().unwrap_or("inside") {
        "inside" => {
            anyhow::ensure!(
                p.within_km.is_none(),
                "`within_km` only applies to `op: nearest`"
            );
            SpatialOp::Inside
        }
        "nearest" => SpatialOp::Nearest {
            within_km: p.within_km,
        },
        other => anyhow::bail!("unknown `op` `{other}`; use `inside` or `nearest`"),
    };
    let points = resolve(ctx, &p.points)?;
    let cols = point_cols(&points).ok_or_else(|| {
        anyhow::anyhow!("`points` has no latitude/longitude columns and no point geometry")
    })?;
    let tables = p
        .layers
        .iter()
        .map(|s| Ok((layer_name(s), resolve(ctx, s)?)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let layers: Vec<Layer> = tables
        .iter()
        .map(|(name, table)| Layer {
            name: name.clone(),
            table,
        })
        .collect();
    let r = spatial_join(&points, cols, &layers, op)?;
    Ok(json!({
        "multi_match": r.multi_match,
        "no_point": r.no_point,
        "table": table_to_json(&r.table, ctx.resolve_row_cap(p.limit), ctx.cell_byte_cap),
    }))
}

pub async fn handle(server: &OctaMcpServer, p: Params) -> Result<CallToolResult, McpError> {
    let ctx = server.tool_context();
    let payload = tokio::task::spawn_blocking(move || run(&ctx, &p))
        .await
        .map_err(|e| McpError::internal_error(format!("join error: {e}"), None))?
        .map_err(|e| McpError::invalid_params(format!("spatial_join failed: {e:#}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(
        payload.to_string(),
    )]))
}
