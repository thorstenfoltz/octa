//! Which tools exist, and what job each belongs to.
//!
//! One catalogue for both surfaces. The GUI assistant uses the groups to send
//! only the core ones up front and fetch the rest on demand
//! (`app::chat::tool_groups`); the MCP server uses them to advertise a subset
//! (`--mcp-tools` / `--mcp-without`) and to drop the write tools under
//! `--mcp-read-only`. It lives here because the tools do: the chat layer
//! already depends on `mcp::tools`, and pointing the dependency the other way
//! would be a cycle.
//!
//! `every_tool_is_catalogued` (in the chat tool tests) pins this against the
//! registry, so a new tool cannot quietly go missing from either surface.

/// A bundle of tools the model can pull in with one `enable_tools` call.
///
/// Grouping is by the job a person came to do, not by implementation: the
/// model picks from this menu having read the user's question, so "I need to
/// compare two files" has to land on one obvious name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolGroup {
    /// Sent with every request. Reading, describing, searching, SQL: the tools
    /// that answer most questions on their own.
    Core,
    Quality,
    Compare,
    Combine,
    Reshape,
    Databases,
    Cloud,
    Write,
}

impl ToolGroup {
    /// Every group, in the order the settings list and the menu show them.
    pub const ALL: &'static [ToolGroup] = &[
        ToolGroup::Core,
        ToolGroup::Quality,
        ToolGroup::Compare,
        ToolGroup::Combine,
        ToolGroup::Reshape,
        ToolGroup::Databases,
        ToolGroup::Cloud,
        ToolGroup::Write,
    ];

    /// The wire name the model passes to `enable_tools`, and the settings key.
    pub fn id(self) -> &'static str {
        match self {
            ToolGroup::Core => "core",
            ToolGroup::Quality => "quality",
            ToolGroup::Compare => "compare",
            ToolGroup::Combine => "combine",
            ToolGroup::Reshape => "reshape",
            ToolGroup::Databases => "databases",
            ToolGroup::Cloud => "cloud",
            ToolGroup::Write => "write",
        }
    }

    /// What the group is for, in one line. Shown to the model in the
    /// `enable_tools` menu and to the user above the group in Settings.
    pub fn summary(self) -> &'static str {
        match self {
            ToolGroup::Core => "reading, describing, searching and SQL over what is open",
            ToolGroup::Quality => "duplicates, outliers, PII, rules and correlations",
            ToolGroup::Compare => "schemas, row-level differences and drift between two sources",
            ToolGroup::Combine => "unions, joins, join keys and reconciling column names",
            ToolGroup::Reshape => "pivot, resample, rolling windows, dedupe, impute, anonymise",
            ToolGroup::Databases => "live database connections, their tables and queries",
            ToolGroup::Cloud => "objects in S3, Azure and GCS",
            ToolGroup::Write => "writing files, editing tabs, converting, charts and reports",
        }
    }

    /// The group a tool belongs to, or `None` for a name no longer known.
    pub fn of(tool: &str) -> Option<ToolGroup> {
        CATALOG.iter().find(|e| e.name == tool).map(|e| e.group)
    }

    pub fn parse(id: &str) -> Option<ToolGroup> {
        ToolGroup::ALL.iter().copied().find(|g| g.id() == id)
    }
}

/// One tool's place in the catalogue.
pub struct ToolEntry {
    pub name: &'static str,
    pub group: ToolGroup,
    /// When a person would want this tool switched on, in one line. English
    /// only, like the tool descriptions themselves: both are written for the
    /// model, and the settings list shows the same words rather than a
    /// translation that could drift from what the model was told.
    pub when: &'static str,
    /// Creates or changes a file, a tab or a database. Both surfaces read this
    /// instead of keeping their own list of write tools.
    pub is_write: bool,
}

const fn e(name: &'static str, group: ToolGroup, when: &'static str) -> ToolEntry {
    ToolEntry {
        name,
        group,
        when,
        is_write: false,
    }
}

/// Same, for a tool that writes.
const fn w(name: &'static str, group: ToolGroup, when: &'static str) -> ToolEntry {
    ToolEntry {
        name,
        group,
        when,
        is_write: true,
    }
}

use ToolGroup::{Cloud, Combine, Compare, Core, Databases, Quality, Reshape, Write};

/// Every tool the assistant can call, with its group and its "when".
///
/// `every_tool_is_catalogued` keeps this in step with the `define_chat_tools!`
/// list, so a new tool cannot quietly go missing from the settings list or the
/// menu.
pub static CATALOG: &[ToolEntry] = &[
    // --- core: always sent -------------------------------------------------
    e(
        "read_table",
        Core,
        "Reading rows out of an open tab or a file.",
    ),
    e(
        "tail",
        Core,
        "Looking at the end of a file without loading it all.",
    ),
    e(
        "sample",
        Core,
        "A quick random look at what the data holds.",
    ),
    e(
        "schema",
        Core,
        "Column names and types. The usual first step.",
    ),
    e(
        "list_tables",
        Core,
        "Sheets in a workbook, tables in a database file.",
    ),
    e(
        "count_rows",
        Core,
        "How many rows, exactly, without reading them.",
    ),
    e(
        "run_sql",
        Core,
        "Any aggregation, filter or join over open data.",
    ),
    e("search", Core, "Finding a value somewhere in a table."),
    e(
        "profile",
        Core,
        "Per-column statistics, nulls and distributions.",
    ),
    e(
        "value_frequency",
        Core,
        "How often each value occurs in a column.",
    ),
    e(
        "describe_file",
        Core,
        "What a file is, before opening it properly.",
    ),
    e(
        "read_text",
        Core,
        "Reading a text, code or Markdown file's content.",
    ),
    e(
        "grep_files",
        Core,
        "Searching many files at once for a pattern.",
    ),
    // --- quality -----------------------------------------------------------
    e("find_duplicates", Quality, "Exact duplicate rows or keys."),
    e(
        "fuzzy_duplicates",
        Quality,
        "Near-duplicates: typos and spelling variants.",
    ),
    e(
        "unique_columns",
        Quality,
        "Finding which columns could serve as a key.",
    ),
    e(
        "detect_outliers",
        Quality,
        "Values far outside the usual range.",
    ),
    e(
        "detect_pii",
        Quality,
        "Spotting personal data before sharing a file.",
    ),
    e(
        "check_rules",
        Quality,
        "Testing data against rules you state.",
    ),
    e(
        "validate_against_schema",
        Quality,
        "Checking a file against a schema file.",
    ),
    e(
        "check_references",
        Quality,
        "Whether keys in one table exist in another.",
    ),
    e(
        "correlation",
        Quality,
        "How strongly numeric columns move together.",
    ),
    e(
        "compare_distributions",
        Quality,
        "Whether two columns are shaped alike.",
    ),
    // --- compare -----------------------------------------------------------
    e(
        "compare_schemas",
        Compare,
        "Column-level differences between two sources.",
    ),
    e(
        "diff_tables",
        Compare,
        "Row-level differences between two tables.",
    ),
    e(
        "data_drift",
        Compare,
        "Whether values have shifted between two versions.",
    ),
    e(
        "schema_drift",
        Compare,
        "Whether the structure changed between versions.",
    ),
    e(
        "export_schema",
        Compare,
        "Writing a schema out to reuse or check against.",
    ),
    // --- combine -----------------------------------------------------------
    e(
        "union_tables",
        Combine,
        "Stacking tables with the same columns.",
    ),
    e(
        "join_tables",
        Combine,
        "Joining tables on a key, written to a file.",
    ),
    e(
        "fuzzy_join",
        Combine,
        "Joining on names that nearly, but not exactly, match.",
    ),
    e(
        "suggest_join_keys",
        Combine,
        "Working out which columns two tables share.",
    ),
    e(
        "diagnose_join",
        Combine,
        "Explaining why a join lost or multiplied rows.",
    ),
    w(
        "harmonise_schemas",
        Combine,
        "Making differing column names line up.",
    ),
    // --- reshape -----------------------------------------------------------
    e("pivot", Reshape, "Turning rows into columns and back."),
    e(
        "resample_timeseries",
        Reshape,
        "Rolling timestamps up to hours, days, months.",
    ),
    e(
        "rolling_window",
        Reshape,
        "Moving averages and running totals.",
    ),
    e(
        "drop_duplicates",
        Reshape,
        "Removing duplicate rows into a new file.",
    ),
    e("fill_missing", Reshape, "Filling gaps in a column."),
    w(
        "transform_columns",
        Reshape,
        "Bulk edits: trim, case, rename, cast.",
    ),
    w(
        "anonymize",
        Reshape,
        "Hashing or masking columns before sharing.",
    ),
    w(
        "partition_table",
        Reshape,
        "Splitting one file into many by a column.",
    ),
    // --- databases ---------------------------------------------------------
    e(
        "list_db_connections",
        Databases,
        "Working with your saved database connections.",
    ),
    e(
        "list_db_tables",
        Databases,
        "Browsing the tables on a database server.",
    ),
    e(
        "db_relationships",
        Databases,
        "Reading the foreign keys a database declares.",
    ),
    e(
        "query_db",
        Databases,
        "Running SQL against a live database server.",
    ),
    e(
        "sync_sql",
        Databases,
        "Keeping a query's result in step with its source.",
    ),
    w(
        "write_db_table",
        Databases,
        "Writing a table back to a database server.",
    ),
    w(
        "copy_db_table",
        Databases,
        "Copying a table between two servers.",
    ),
    // --- cloud -------------------------------------------------------------
    e(
        "list_objects",
        Cloud,
        "Listing what is in an S3, Azure or GCS bucket.",
    ),
    w(
        "copy_object",
        Cloud,
        "Copying an object inside cloud storage.",
    ),
    w(
        "move_object",
        Cloud,
        "Moving or renaming an object in cloud storage.",
    ),
    w(
        "delete_object",
        Cloud,
        "Deleting an object from cloud storage.",
    ),
    // --- write -------------------------------------------------------------
    w(
        "write_table",
        Write,
        "Saving data the assistant produced to a file.",
    ),
    w(
        "write_workbook",
        Write,
        "Writing several sheets into one Excel file.",
    ),
    w(
        "write_text",
        Write,
        "Saving edited text, code or Markdown back to disk.",
    ),
    w(
        "edit_table",
        Write,
        "Editing a file on disk that is not open.",
    ),
    w(
        "edit_open_tab",
        Write,
        "Editing the tab you are looking at, undoably.",
    ),
    w(
        "convert",
        Write,
        "Transcoding a whole file to another format.",
    ),
    w(
        "batch_convert",
        Write,
        "Converting a directory of files in one go.",
    ),
    w(
        "create_report",
        Write,
        "Producing an HTML report from a source.",
    ),
    w("create_chart", Write, "Drawing a chart into a new tab."),
];

/// Every tool name in a group, in catalogue order.
pub fn names_in(group: ToolGroup) -> Vec<&'static str> {
    CATALOG
        .iter()
        .filter(|entry| entry.group == group)
        .map(|entry| entry.name)
        .collect()
}

/// Resolve one `--mcp-tools` / `--mcp-without` item: a group name expands to
/// its tools, a tool name stands for itself.
///
/// `Err` carries what the user typed, so the caller can name it and list what
/// would have worked instead of starting a server that quietly exposes the
/// wrong surface.
pub fn resolve_selector(item: &str) -> Result<Vec<&'static str>, String> {
    let item = item.trim();
    if let Some(group) = ToolGroup::parse(item) {
        return Ok(names_in(group));
    }
    if let Some(entry) = CATALOG.iter().find(|e| e.name == item) {
        return Ok(vec![entry.name]);
    }
    Err(item.to_string())
}

/// Which tool names to hide, given the `--mcp-tools` / `--mcp-without` lists.
///
/// An empty `only` means "everything"; otherwise the catalogue minus what was
/// asked for. `without` is applied afterwards, so the two combine. An
/// unrecognised word in either list is an error naming it and every valid
/// group, because starting a server that silently exposes the wrong surface is
/// the one outcome worth refusing.
pub fn hidden_tools(only: &[String], without: &[String]) -> Result<Vec<&'static str>, String> {
    let mut keep: Vec<&'static str> = if only.is_empty() {
        CATALOG.iter().map(|e| e.name).collect()
    } else {
        let mut names = Vec::new();
        for item in only {
            names.extend(resolve_selector(item).map_err(bad_selector)?);
        }
        names
    };
    for item in without {
        let drop = resolve_selector(item).map_err(bad_selector)?;
        keep.retain(|name| !drop.contains(name));
    }
    Ok(CATALOG
        .iter()
        .map(|e| e.name)
        .filter(|name| !keep.contains(name))
        .collect())
}

fn bad_selector(item: String) -> String {
    let groups: Vec<&str> = ToolGroup::ALL.iter().map(|g| g.id()).collect();
    format!(
        "unknown tool or group `{item}`. Groups: {}. Tool names are the ones \
         `tools/list` reports, e.g. read_table.",
        groups.join(", ")
    )
}

/// The tools that create or change a file, a tab or a database. `--mcp-read-only`
/// drops exactly these, and a chat profile without **Allow writes** never sees
/// them. Derived from the group rather than a second hand-kept list: everything
/// in `Write`, `Cloud` (bar the listing) and the two database writers.
pub fn write_tool_names() -> Vec<&'static str> {
    CATALOG
        .iter()
        .filter(|e| e.is_write)
        .map(|e| e.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_lists_hides_nothing() {
        assert!(hidden_tools(&[], &[]).unwrap().is_empty());
    }

    #[test]
    fn a_group_keeps_only_its_own_tools() {
        let hidden = hidden_tools(&["core".to_string()], &[]).unwrap();
        assert!(!hidden.contains(&"read_table"), "core survives");
        assert!(hidden.contains(&"fuzzy_join"), "everything else is hidden");
    }

    #[test]
    fn a_tool_name_works_on_its_own_and_mixes_with_groups() {
        let hidden = hidden_tools(&["cloud".to_string(), "run_sql".to_string()], &[]).unwrap();
        assert!(!hidden.contains(&"list_objects"));
        assert!(!hidden.contains(&"run_sql"));
        assert!(hidden.contains(&"read_table"));
    }

    #[test]
    fn without_subtracts_from_what_survived() {
        let hidden = hidden_tools(&["core".to_string()], &["run_sql".to_string()]).unwrap();
        assert!(hidden.contains(&"run_sql"));
        assert!(!hidden.contains(&"read_table"));
        // On its own it just removes that group.
        let hidden = hidden_tools(&[], &["cloud".to_string()]).unwrap();
        assert!(hidden.contains(&"delete_object"));
        assert!(!hidden.contains(&"read_table"));
    }

    /// Stray whitespace around a name is the shell's, not the user's mistake.
    #[test]
    fn surrounding_whitespace_is_ignored() {
        let hidden = hidden_tools(&[" quality ".to_string()], &[]).expect("trimmed");
        assert!(!hidden.contains(&"detect_pii"));
    }

    #[test]
    fn an_unknown_word_names_the_groups() {
        let err = hidden_tools(&["qualityy".to_string()], &[]).unwrap_err();
        assert!(err.contains("qualityy"), "{err}");
        assert!(err.contains("quality"), "{err}");
        assert!(err.contains("databases"), "{err}");
    }

    /// Every tool the server advertises is in the site's tool reference, with
    /// the group `--mcp-tools` expects. A tool added without a row there would
    /// otherwise be invisible to anyone configuring the server, since the MCP
    /// user has no Settings list to look at.
    #[test]
    fn the_docs_table_lists_every_tool_with_its_group() {
        let server = crate::mcp::OctaMcpServer::new(Some(1000), 65536, false, false, true, 0, &[]);
        let doc = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/mcp/tools/index.md"),
        )
        .expect("the tool reference page");

        for tool in server.tool_router.list_all() {
            let name = tool.name.as_ref();
            let group = ToolGroup::of(name)
                .unwrap_or_else(|| panic!("`{name}` is served but not in the catalogue"))
                .id();
            let row = doc
                .lines()
                .find(|l| l.starts_with(&format!("| **[`{name}`]")))
                .unwrap_or_else(|| panic!("`{name}` has no row in docs/mcp/tools/index.md"));
            assert!(
                row.contains(&format!("`{group}`")),
                "`{name}` is in group `{group}`, but its docs row says otherwise: {row}"
            );
        }
    }

    /// And nothing in the table is made up.
    #[test]
    fn the_docs_table_invents_no_tools() {
        let server = crate::mcp::OctaMcpServer::new(Some(1000), 65536, false, false, true, 0, &[]);
        let doc = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/mcp/tools/index.md"),
        )
        .expect("the tool reference page");
        for line in doc.lines().filter(|l| l.starts_with("| **[`")) {
            let name = line
                .trim_start_matches("| **[`")
                .split('`')
                .next()
                .expect("a tool name");
            assert!(
                server.tool_router.has_route(name),
                "docs/mcp/tools/index.md lists `{name}`, which the server does not serve"
            );
        }
    }
}
