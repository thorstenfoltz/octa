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
            match name {
                $(
                    $name => run_typed(ctx, args, $module::run),
                )+
                other => Err(format!("unknown tool: {other}")),
            }
        }
    };
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
    "value_frequency"          => value_frequency,
    "search"                   => search,
    "compare_schemas"          => compare_schemas,
    "diff_tables"              => diff_tables,
    "validate_against_schema"  => validate_schema,
    "describe_file"            => describe_file,
    "unique_columns"           => unique_columns,
    "write_table"              => write_table,
    "edit_table"               => edit_table,
    "create_chart"             => create_chart,
    "read_text"                => read_text,
    "write_text"               => write_text,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::{Source, TableSnapshot};
    use octa::data::{CellValue, ColumnInfo, DataTable};

    fn sample_ctx() -> ToolContext {
        let mut table = DataTable::empty();
        table.columns = vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "Utf8".into(),
            },
        ];
        table.rows = vec![
            vec![CellValue::Int(1), CellValue::String("a".into())],
            vec![CellValue::Int(2), CellValue::String("b".into())],
        ];
        ToolContext {
            open_tabs: vec![TableSnapshot {
                handle: "#1".into(),
                display_name: "demo".into(),
                source_path: None,
                table,
            }],
            active_tab: Some(0),
            default_row_limit: Some(1000),
            cell_byte_cap: 65_536,
            restrict_filesystem: false,
            allowed_read_paths: Vec::new(),
            export_dir: None,
        }
    }

    /// A single-column (`id`) tab, for multi-tab JOIN tests.
    fn id_tab(handle: &str, name: &str, ids: &[i64]) -> TableSnapshot {
        let mut table = DataTable::empty();
        table.columns = vec![ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        }];
        table.rows = ids.iter().map(|i| vec![CellValue::Int(*i)]).collect();
        TableSnapshot {
            handle: handle.into(),
            display_name: name.into(),
            source_path: None,
            table,
        }
    }

    fn multi_ctx() -> ToolContext {
        ToolContext {
            open_tabs: vec![
                id_tab("#1", "a.csv", &[1, 2, 3]),
                id_tab("#2", "b.csv", &[2, 3, 4]),
                id_tab("#3", "c.csv", &[3, 4, 5]),
            ],
            active_tab: Some(0),
            default_row_limit: Some(1000),
            cell_byte_cap: 65_536,
            restrict_filesystem: false,
            allowed_read_paths: Vec::new(),
            export_dir: None,
        }
    }

    #[test]
    fn path_addresses_open_tab_by_handle_or_name() {
        let ctx = sample_ctx();
        // The model often puts the handle / file name in `path` instead of
        // `open_tab` - both resolve to the open tab.
        let by_handle = dispatch(&ctx, "schema", serde_json::json!({"path": "#1"}))
            .expect("path handle resolves");
        assert_eq!(by_handle["column_count"], 2);
        let by_name = dispatch(&ctx, "schema", serde_json::json!({"path": "demo"}))
            .expect("path name resolves");
        assert_eq!(by_name["column_count"], 2);
    }

    #[test]
    fn run_sql_joins_multiple_open_tabs() {
        let ctx = multi_ctx();
        let out = dispatch(
            &ctx,
            "run_sql",
            serde_json::json!({
                "open_tab": "#1",
                "query": "SELECT count(*) AS n FROM data JOIN b USING(id) JOIN c USING(id)",
                "extra_tables": [{"name": "b", "path": "#2"}, {"name": "c", "path": "#3"}]
            }),
        )
        .expect("3-way join across open tabs");
        assert_eq!(out["kind"], "select");
        // ids common to a{1,2,3}, b{2,3,4}, c{3,4,5} = {3} -> exactly 1 row.
        assert_eq!(out["result"]["rows"][0][0], serde_json::json!(1));
    }

    #[test]
    fn every_tool_has_a_schema_and_description() {
        for def in tool_defs() {
            assert!(!def.name.is_empty());
            assert!(
                !def.description.is_empty(),
                "tool {} has an empty description",
                def.name
            );
            assert_eq!(def.input_schema["type"], "object", "tool {}", def.name);
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let names: Vec<String> = tool_defs().into_iter().map(|d| d.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "duplicate tool name");
    }

    #[test]
    fn dispatch_schema_against_active_tab() {
        let ctx = sample_ctx();
        let out = dispatch(&ctx, "schema", serde_json::json!({"open_tab": "@active"}))
            .expect("schema dispatch");
        assert_eq!(out["column_count"], 2);
    }

    #[test]
    fn dispatch_run_sql_against_named_tab() {
        let ctx = sample_ctx();
        let out = dispatch(
            &ctx,
            "run_sql",
            serde_json::json!({"open_tab": "demo", "query": "SELECT count(*) AS n FROM data"}),
        )
        .expect("run_sql dispatch");
        assert_eq!(out["kind"], "select");
        // The single result cell is the row count.
        assert_eq!(out["result"]["rows"][0][0], serde_json::json!(2));
    }

    #[test]
    fn run_sql_write_to_csv_file() {
        let ctx = sample_ctx();
        let path = std::env::temp_dir().join("octa_run_sql_write_test.csv");
        let _ = std::fs::remove_file(&path);
        let out = dispatch(
            &ctx,
            "run_sql",
            serde_json::json!({
                "open_tab": "demo",
                "query": "SELECT * FROM data",
                "write_to": { "path": path.to_str().unwrap(), "table": "ignored" }
            }),
        )
        .expect("run_sql write_to csv");
        assert_eq!(out["kind"], "write_back");
        assert_eq!(out["rows_written"], serde_json::json!(2));
        let written = std::fs::read_to_string(&path).expect("csv written");
        // Header + 2 data rows.
        assert_eq!(written.lines().count(), 3);
        assert!(written.contains("id"));
        let _ = std::fs::remove_file(&path);
    }

    /// A single "Line" column tab, the shape text/code/markdown files load as.
    fn text_ctx(lines: &[&str], source_path: Option<&str>) -> ToolContext {
        let mut table = DataTable::empty();
        table.columns = vec![ColumnInfo {
            name: "Line".into(),
            data_type: "Utf8".into(),
        }];
        table.rows = lines
            .iter()
            .map(|l| vec![CellValue::String((*l).into())])
            .collect();
        ToolContext {
            open_tabs: vec![TableSnapshot {
                handle: "#1".into(),
                display_name: "notes.md".into(),
                source_path: source_path.map(|s| s.to_string()),
                table,
            }],
            active_tab: Some(0),
            default_row_limit: Some(1000),
            cell_byte_cap: 65_536,
            restrict_filesystem: false,
            allowed_read_paths: Vec::new(),
            export_dir: None,
        }
    }

    #[test]
    fn read_text_rejoins_lines() {
        let ctx = text_ctx(&["# Title", "", "body line"], None);
        let out = dispatch(
            &ctx,
            "read_text",
            serde_json::json!({"open_tab": "@active"}),
        )
        .expect("read_text dispatch");
        assert_eq!(out["text"], "# Title\n\nbody line");
        assert_eq!(out["line_count"], 3);
    }

    #[test]
    fn write_text_writes_file() {
        let ctx = text_ctx(&["old"], None);
        let path = std::env::temp_dir().join("octa_write_text_test.md");
        let _ = std::fs::remove_file(&path);
        let out = dispatch(
            &ctx,
            "write_text",
            serde_json::json!({"content": "new content\n", "path": path.to_str().unwrap()}),
        )
        .expect("write_text dispatch");
        assert_eq!(out["bytes_written"], serde_json::json!(12));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_tool_errors_cleanly() {
        let ctx = sample_ctx();
        let err = dispatch(&ctx, "nope", serde_json::json!({})).unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    #[test]
    fn write_back_to_open_tab_is_rejected() {
        let ctx = sample_ctx();
        // The Source enum is part of the shared API the chat layer builds on.
        let _ = Source::ActiveTab;
        let err = dispatch(
            &ctx,
            "write_table",
            serde_json::json!({"path": "/tmp/x.csv", "open_tab": "demo", "columns": [{"name": "a"}]}),
        )
        .unwrap_err();
        assert!(err.contains("not supported"));
    }
}
