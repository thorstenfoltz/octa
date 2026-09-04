//! Chat tool registry: turns the MCP tools' `schemars`-derived `Params` into
//! LLM tool definitions and dispatches a model-issued tool call to the same
//! `crate::mcp::tools::<name>::run` the MCP server uses. No parallel tool set
//! exists - both surfaces share one implementation.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::mcp::tools::ToolContext;

use super::tool_groups;
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
            if ctx.read_only && write_tool_names().contains(&name) {
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
///
/// One list, in the catalogue, shared with `--mcp-read-only`.
pub fn write_tool_names() -> Vec<&'static str> {
    tool_groups::write_tool_names()
}

/// The name of the meta-tool that fetches a group of tools mid-turn.
pub const ENABLE_TOOLS: &str = "enable_tools";

/// Every tool a profile may use: the whole set, minus the write tools when the
/// profile does not allow writes, minus whatever the user switched off in
/// Settings. This is the pool the other two functions draw from, and what
/// `dispatch` checks a call against.
pub fn tool_defs_for(allow_writes: bool, disabled: &BTreeSet<String>) -> Vec<ToolDef> {
    let mut defs = tool_defs();
    if !allow_writes {
        defs.retain(|d| !write_tool_names().contains(&d.name.as_str()));
    }
    defs.retain(|d| !disabled.contains(&d.name));
    defs
}

/// What actually goes out with the first request: the core group in full, plus
/// the `enable_tools` menu when any other tool is still available.
///
/// The menu names the remaining tools without describing them, which is what
/// makes this work: the model can see that `fuzzy_join` exists and ask for it,
/// without the ~19k tokens the other groups' schemas would cost on every
/// request whether or not they are ever used.
pub fn initial_tool_defs(allow_writes: bool, disabled: &BTreeSet<String>) -> Vec<ToolDef> {
    let available = tool_defs_for(allow_writes, disabled);
    let mut defs: Vec<ToolDef> = available
        .iter()
        .filter(|d| tool_groups::ToolGroup::of(&d.name) == Some(tool_groups::ToolGroup::Core))
        .cloned()
        .collect();
    if let Some(menu) = enable_tools_def(allow_writes, disabled) {
        defs.push(menu);
    }
    defs
}

/// The `enable_tools` definition, or `None` when every non-core tool is off
/// and there is nothing left to fetch.
pub fn enable_tools_def(allow_writes: bool, disabled: &BTreeSet<String>) -> Option<ToolDef> {
    let mut menu = String::new();
    let mut ids: Vec<String> = Vec::new();
    for group in tool_groups::ToolGroup::ALL {
        if *group == tool_groups::ToolGroup::Core {
            continue;
        }
        let members = tool_groups::group_members(*group, disabled, allow_writes);
        if members.is_empty() {
            continue;
        }
        ids.push(group.id().to_string());
        menu.push_str(&format!(
            "\n- {} ({}): {}",
            group.id(),
            group.summary(),
            members.join(", ")
        ));
    }
    if ids.is_empty() {
        return None;
    }
    Some(ToolDef {
        name: ENABLE_TOOLS.to_string(),
        description: format!(
            "Load more tools for the rest of this conversation. The tools listed \
below are available but not yet loaded, so their parameters are not visible \
to you yet. When the job needs one of them, call this with that group's name \
and it becomes callable straight away - do NOT tell the user a capability is \
missing without fetching its group first. Ask for the one group you need, not \
for all of them.\n\nGroups:{menu}"
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "groups": {
                    "type": "array",
                    "items": { "type": "string", "enum": ids },
                    "description": "Group names to load, e.g. [\"combine\"].",
                }
            },
            "required": ["groups"],
        }),
    })
}

/// Answer an `enable_tools` call: the groups it asked for, as definitions to
/// append to the live turn, plus the sentence the model gets back.
///
/// Unknown group names are reported rather than ignored, so a model that
/// guesses gets told what the real names are instead of silently getting
/// nothing.
pub fn enable_groups(
    args: &Value,
    allow_writes: bool,
    disabled: &BTreeSet<String>,
    already: &[ToolDef],
) -> (Vec<ToolDef>, String) {
    let requested: Vec<String> = args["groups"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let pool = tool_defs_for(allow_writes, disabled);
    let mut added: Vec<ToolDef> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for id in &requested {
        let Some(group) = tool_groups::ToolGroup::parse(id) else {
            unknown.push(id.clone());
            continue;
        };
        for name in tool_groups::group_members(group, disabled, allow_writes) {
            if already.iter().any(|d| d.name == name) || added.iter().any(|d| d.name == name) {
                continue;
            }
            if let Some(def) = pool.iter().find(|d| d.name == name) {
                added.push(def.clone());
                names.push(name.to_string());
            }
        }
    }
    let mut msg = if names.is_empty() {
        "nothing new was loaded".to_string()
    } else {
        format!("loaded and ready to call: {}", names.join(", "))
    };
    if !unknown.is_empty() {
        let valid: Vec<&str> = tool_groups::ToolGroup::ALL
            .iter()
            .filter(|g| **g != tool_groups::ToolGroup::Core)
            .map(|g| g.id())
            .collect();
        msg.push_str(&format!(
            "; no such group: {} (the groups are {})",
            unknown.join(", "),
            valid.join(", ")
        ));
    }
    (added, msg)
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
    "schema_drift"             => schema_drift,
    "data_drift"               => data_drift,
    "check_rules"              => check_rules,
    "create_report"            => create_report,
    "fuzzy_join"               => fuzzy_join,
    "suggest_join_keys"        => suggest_join_keys,
    "diagnose_join"            => diagnose_join,
    "harmonise_schemas"        => harmonise_schemas,
    "pivot"                    => pivot,
    "batch_convert"            => batch_convert,
    "resample_timeseries"      => resample,
    "rolling_window"           => rolling_window,
    "correlation"              => correlation,
    "compare_distributions"    => compare_distributions,
    "check_references"         => check_references,
    "grep_files"               => grep_files,
    "list_objects"             => list_objects,
    "copy_object"              => copy_object,
    "move_object"              => move_object,
    "delete_object"            => delete_object,
    "list_db_connections"      => list_db_connections,
    "list_db_tables"           => list_db_tables,
    "db_relationships"         => db_relationships,
    "query_db"                 => query_db,
    "sync_sql"                 => sync_sql,
    "write_workbook"           => write_workbook,
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
