//! Chat tool registry: turns the MCP tools' `schemars`-derived `Params` into
//! LLM tool definitions and dispatches a model-issued tool call to the same
//! `crate::mcp::tools::<name>::run` the MCP server uses. No parallel tool set
//! exists - both surfaces share one implementation.

use serde_json::{Map, Value};

use crate::mcp::tools::ToolContext;

use super::types::ToolDef;

/// Deserialize `args` into the tool's `Params`, run it, and stringify any
/// error so the model can read and recover from it rather than the turn
/// aborting.
fn run_typed<P>(
    ctx: &ToolContext,
    args: Value,
    f: fn(&ToolContext, &P) -> anyhow::Result<Value>,
) -> Result<Value, String>
where
    P: serde::de::DeserializeOwned,
{
    let p: P = serde_json::from_value(args).map_err(|e| format!("invalid arguments: {e}"))?;
    f(ctx, &p).map_err(|e| e.to_string())
}

/// Strip schema keys that some providers reject (`$schema`, `title`) and make
/// sure the top level is an object with `type: "object"`.
fn normalize_schema(mut v: Value) -> Value {
    if let Value::Object(map) = &mut v {
        map.remove("$schema");
        map.remove("title");
        map.entry("type")
            .or_insert_with(|| Value::String("object".to_string()));
        // Anthropic / OpenAI tolerate an empty `properties`, but a few
        // OpenAI-compatible servers choke on its absence; ensure it exists.
        map.entry("properties")
            .or_insert_with(|| Value::Object(Map::new()));
    }
    v
}

/// Generate the LLM tool definitions and the dispatch table from one list, so
/// the two can never drift. Each entry pairs the wire tool name with the MCP
/// tool module that provides `Params`, `DESCRIPTION`, and `run`.
macro_rules! define_chat_tools {
    ($( $name:literal => $module:ident ),+ $(,)?) => {
        use crate::mcp::tools::{$($module),+};

        /// The tools advertised to the model, in a stable order.
        pub fn tool_defs() -> Vec<ToolDef> {
            vec![
                $(
                    ToolDef {
                        name: $name.to_string(),
                        description: $module::DESCRIPTION.to_string(),
                        input_schema: normalize_schema(
                            serde_json::to_value(schemars::schema_for!($module::Params))
                                .unwrap_or(Value::Null),
                        ),
                    },
                )+
            ]
        }

        /// Run a model-issued tool call against the shared `ToolContext`.
        pub fn dispatch(ctx: &ToolContext, name: &str, args: Value) -> Result<Value, String> {
            if ctx.read_only && WRITE_TOOL_NAMES.contains(&name) {
                return Err(
                    "writes are disabled: this chat profile does not allow writes \
                     (enable \"Allow writes\" on the profile in Settings)"
                        .to_string(),
                );
            }
            match name {
                $(
                    $name => run_typed(ctx, args, $module::run),
                )+
                other => Err(format!("unknown tool: {other}")),
            }
        }
    };
}

/// Tools that create or mutate files, open tabs, or databases. Hidden from
/// the model and refused by `dispatch` when the profile disallows writes
/// (`ctx.read_only`).
pub const WRITE_TOOL_NAMES: &[&str] = &[
    "write_table",
    "edit_table",
    "edit_open_tab",
    "convert",
    "transform_columns",
    "anonymize",
    "partition_table",
    "write_db_table",
    "copy_db_table",
    "write_text",
    "create_chart",
];

/// The tool list for a profile: everything, minus the write tools when the
/// profile does not allow writes.
pub fn tool_defs_for(allow_writes: bool) -> Vec<ToolDef> {
    let mut defs = tool_defs();
    if !allow_writes {
        defs.retain(|d| !WRITE_TOOL_NAMES.contains(&d.name.as_str()));
    }
    defs
}

define_chat_tools! {
    "read_table"               => read_table,
    "tail"                     => tail,
    "sample"                   => sample,
    "schema"                   => schema,
    "list_tables"              => list_tables,
    "count_rows"               => count_rows,
    "run_sql"                  => run_sql,
    "convert"                  => convert,
    "export_schema"            => export_schema,
    "profile"                  => profile,
    "find_duplicates"          => find_duplicates,
    "fuzzy_duplicates"         => fuzzy_duplicates,
    "value_frequency"          => value_frequency,
    "search"                   => search,
    "compare_schemas"          => compare_schemas,
    "diff_tables"              => diff_tables,
    "union_tables"             => union,
    "join_tables"              => join,
    "drop_duplicates"          => dedupe,
    "fill_missing"             => impute,
    "validate_against_schema"  => validate_schema,
    "describe_file"            => describe_file,
    "unique_columns"           => unique_columns,
    "pivot"                    => pivot,
    "correlation"              => correlation,
    "grep_files"               => grep_files,
    "list_objects"             => list_objects,
    "list_db_connections"      => list_db_connections,
    "list_db_tables"           => list_db_tables,
    "query_db"                 => query_db,
    "write_db_table"           => write_db_table,
    "copy_db_table"            => copy_db_table,
    "write_table"              => write_table,
    "edit_table"               => edit_table,
    "edit_open_tab"            => edit_open_tab,
    "transform_columns"        => transform_columns,
    "anonymize"                => anonymize,
    "partition_table"          => partition,
    "detect_outliers"          => outliers,
    "detect_pii"               => pii,
    "create_chart"             => create_chart,
    "read_text"                => read_text,
    "write_text"               => write_text,
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
