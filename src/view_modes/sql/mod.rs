use crate::app::state::TabState;

use eframe::egui;
use octa::data::CellValue;
use octa::ui::control_row::control_height;
use octa::ui::settings::SqlEditorFont;
use octa::ui::status_bar::format_number;

/// Resolve the configured `SqlEditorFont` into an `egui::FontFamily`. The
/// `JetBrainsMono` variant points at the bundled named family registered in
/// `apply_fonts`; `MatchUiFont` falls back to the active style's body font;
/// `SystemMonospace` uses egui's built-in mono family.
fn sql_font_family(font: SqlEditorFont, ui: &egui::Ui) -> egui::FontFamily {
    match font {
        SqlEditorFont::JetBrainsMono => egui::FontFamily::Name(std::sync::Arc::from("sql_mono")),
        SqlEditorFont::SystemMonospace => egui::FontFamily::Monospace,
        SqlEditorFont::MatchUiFont => ui.style().text_styles[&egui::TextStyle::Body]
            .family
            .clone(),
    }
}

/// User actions emitted by the SQL view in a single frame. The fields beyond
/// `run` / `clear` / `export` / `close` drive the per-tab SQL workspace:
/// adding extra tables, ATTACHing databases, removing them, and writing
/// query results back to a DuckDB or SQLite file.
#[derive(Debug, Clone, Default)]
pub struct SqlAction {
    pub run: bool,
    pub clear: bool,
    /// User chose **Clear history** in the History menu: forget every recorded
    /// query for this tab's connection or file.
    pub clear_history: bool,
    pub export: bool,
    /// User clicked the × button in the panel header. The caller flips
    /// `tab.sql_panel_open` to false, hiding the panel until the user
    /// reopens it from **Analyse -> SQL**.
    pub close: bool,
    /// User clicked **+ Add table...**. Opens a multi-file picker that
    /// registers every chosen file under a sanitised, de-duplicated name.
    pub add_tables: bool,
    /// User clicked **Attach database...**. Opens a single-file picker
    /// (DuckDB / SQLite) and ATTACHes the file under a default alias.
    pub attach_db: bool,
    /// User clicked **[refresh]** next to `data` (or wants the workspace
    /// to re-register the active tab's table from the live edited state).
    pub refresh_active: bool,
    /// User clicked **[×]** next to a registered table.
    pub remove_table: Option<String>,
    /// User clicked **[detach]** next to an ATTACH-ed database.
    pub detach_alias: Option<String>,
    /// User clicked **Write result to DB...**. The panel opens the write-back
    /// dialog which composes the actual `WriteTarget`.
    pub open_write_back: bool,
    /// User clicked **Format**: lay the query out in the style set under
    /// Settings -> SQL.
    pub format: bool,
    /// User clicked **Open result as tab...**: the whole result becomes a
    /// new tab of its own.
    pub result_to_tab: bool,
    /// User pressed Ask with a question in the plain-language box. The panel
    /// fires one request; the answer is spliced into the editor, never run.
    pub ask: Option<String>,
    /// User selected a new entry in the workspace tree (or cleared the
    /// selection). The panel updates `tab.sql_inspector_selection` and
    /// triggers a fresh introspection fetch (cached on `TabState` so the
    /// next frame doesn't re-query).
    pub select_inspector: Option<Option<crate::app::sql_panel::InspectorTarget>>,
    /// User clicked **Insert** in the inspector. The panel appends a SELECT
    /// statement (`SELECT * FROM <qualified> LIMIT 100;`) into the editor.
    pub insert_qualified: Option<String>,
    /// User clicked **Run** in the inspector. The panel replaces the editor
    /// content with `SELECT * FROM <qualified> LIMIT N` and runs it.
    pub run_qualified: Option<String>,
    /// User clicked **Copy name**. The panel copies the qualified name to
    /// the system clipboard.
    pub copy_qualified: Option<String>,
    /// User toggled the open/closed state of an attached schema group.
    /// String is the tree key (`alias` or `alias::schema`).
    pub toggle_tree_key: Option<String>,
    /// User picked a recent query from the History dropdown (the query text).
    pub recall_query: Option<String>,
    /// User picked a saved snippet (the snippet's query text) to load.
    pub insert_snippet: Option<String>,
    /// User dismissed the note about auto-registered open tabs.
    pub dismiss_auto_register_notice: bool,
    /// The result grid of this pane (`SqlPane.id`) scrolled close enough to
    /// the end of the rows it holds that the next page should be fetched.
    pub load_more_rows: Option<u64>,
    /// User clicked **+** to open another editor beside the others.
    pub add_pane: bool,
    /// User closed the editor at this position.
    pub close_pane: Option<usize>,
    /// User clicked Maximise / Restore in the panel header.
    pub toggle_maximise: bool,
    /// User clicked **Save current query as snippet...**.
    pub save_snippet: bool,
    /// User deleted a saved snippet by name.
    pub delete_snippet: Option<String>,
    /// User clicked the **Snippets** button to open the snippet manager window.
    pub open_snippets_window: bool,
    /// User clicked **Cancel** on an in-flight "Run on server" query.
    pub cancel_server: bool,
    /// User picked what to attach from a saved live-database connection:
    /// `(connection id, which part of the server)`. The scope is empty for a
    /// whole two-level server and carries whatever the user drilled into.
    pub attach_db_connection: Option<(String, octa::sql::AttachScope)>,
    /// Node whose children the attach menu needs: `(connection id, path below
    /// the connection root)`. Fetched in the background; the menu shows
    /// "Loading..." until it lands.
    pub list_db_node: Option<(String, Vec<String>)>,
    /// Cloud connection whose object picker should open.
    pub attach_cloud_connection: Option<String>,
    /// User renamed a workspace table in place: `(current name, new name)`.
    pub rename_table: Option<(String, String)>,
}

/// Persistent id of the SQL editor TextEdit. Exposed so the global keyboard
/// handler in `main.rs` can tell whether the editor currently has focus.
/// One per pane (`SqlPane.id`), so each editor keeps its own caret.
pub fn editor_id(pane: u64) -> egui::Id {
    egui::Id::new(("sql_editor", pane))
}

/// What Run executes: the marked part of the editor when something is
/// marked, the whole buffer otherwise. Read from the editor's stored cursor,
/// which survives the click on Run taking focus away from it.
pub fn query_to_run(ctx: &egui::Context, pane: &crate::app::state::SqlPane) -> String {
    super::text_ops::selected_text(ctx, editor_id(pane.id), &pane.query)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| pane.query.clone())
}

/// Byte ranges of the `--` line comments in `sql`, each running to the end
/// of its line (newline excluded). A `--` inside a single-quoted string is
/// text, not a comment; `''` inside a string is an escaped quote.
pub fn line_comment_ranges(sql: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => in_string = !in_string,
            b'-' if !in_string && bytes.get(i + 1) == Some(&b'-') => {
                let end = sql[i..].find('\n').map_or(sql.len(), |n| i + n);
                out.push(i..end);
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// Comment or uncomment the lines a selection touches (byte offsets into
/// `text`). Returns the new text and where the selection ends up, so the
/// same lines stay marked and a second press undoes the first.
///
/// - A line counts as commented when it starts with `--` after its leading
///   blanks. If every non-blank marked line is, the first `--` of each is
///   removed (only those two characters, so `-- -- x` keeps one). Otherwise
///   every non-blank marked line gets `--` in front of its first character.
///   Blank lines are left alone.
/// - A `--` after code (`SELECT a -- note`) does not make the line commented,
///   unless the selection lies entirely inside that comment: then just that
///   `--` is removed, turning the note back into code.
/// - A selection ending at the very start of a line does not include it,
///   the way a dragged full-line selection ends.
pub fn toggle_line_comments(
    text: &str,
    sel: std::ops::Range<usize>,
) -> (String, std::ops::Range<usize>) {
    let line_start = |p: usize| text[..p].rfind('\n').map_or(0, |i| i + 1);
    let line_end = |p: usize| text[p..].find('\n').map_or(text.len(), |i| p + i);

    // A selection inside a trailing comment removes that one `--`.
    if sel.start < sel.end && sel.end <= line_end(sel.start) {
        let (ls, le) = (line_start(sel.start), line_end(sel.start));
        if let Some(c) = line_comment_ranges(text)
            .into_iter()
            .find(|r| r.start >= ls && r.start < le)
            && !text[ls..c.start].trim().is_empty()
            && sel.start >= c.start
        {
            let mut out = text.to_string();
            out.replace_range(c.start..c.start + 2, "");
            let map = |p: usize| {
                if p >= c.start + 2 {
                    p - 2
                } else {
                    p.min(c.start)
                }
            };
            return (out, map(sel.start)..map(sel.end));
        }
    }

    let first = line_start(sel.start);
    let last_pos = if sel.end > sel.start && sel.end > first && text[..sel.end].ends_with('\n') {
        sel.end - 1
    } else {
        sel.end
    };
    let last = line_end(last_pos.max(sel.start));

    // (absolute position of the first non-blank char, is commented) per
    // non-blank line.
    let mut lines: Vec<(usize, bool)> = Vec::new();
    let mut at = first;
    for line in text[first..last].split('\n') {
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        let rest = &line[indent..];
        if !rest.trim().is_empty() {
            lines.push((at + indent, rest.starts_with("--")));
        }
        at += line.len() + 1;
    }
    if lines.is_empty() {
        return (text.to_string(), sel);
    }
    let uncomment = lines.iter().all(|&(_, c)| c);

    let mut out = text.to_string();
    // Back to front, so earlier offsets stay valid while editing.
    for &(p, _) in lines.iter().rev() {
        if uncomment {
            out.replace_range(p..p + 2, "");
        } else {
            out.insert_str(p, "--");
        }
    }
    // `grow`: whether a position sitting exactly where `--` is inserted moves
    // past it. The selection's start does not, so a selection that began at
    // the start of a line still covers its new `--`; its end (and a bare
    // caret) does.
    let map = |pos: usize, grow: bool| {
        lines.iter().fold(pos, |acc, &(p, _)| {
            if uncomment {
                if pos >= p + 2 {
                    acc - 2
                } else if pos > p {
                    acc - (pos - p)
                } else {
                    acc
                }
            } else if pos > p || (grow && pos == p) {
                acc + 2
            } else {
                acc
            }
        })
    };
    let caret = sel.start == sel.end;
    (out, map(sel.start, caret)..map(sel.end, true))
}

/// Apply [`toggle_line_comments`] to the SQL editor: reads its stored
/// selection (the caret alone counts as its line), rewrites the buffer and
/// puts the selection back over the same text.
pub fn toggle_comment_in_editor(ctx: &egui::Context, pane: &mut crate::app::state::SqlPane) {
    use super::text_ops::char_range_to_byte_range;
    let id = editor_id(pane.id);
    let query = &mut pane.query;
    let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    let (a, b) = (range.primary.index.0, range.secondary.index.0);
    let (start, end) = (a.min(b), a.max(b));
    let bytes = char_range_to_byte_range(query, start, end);
    let (new_text, new_sel) = toggle_line_comments(query, bytes);
    if new_text == *query {
        return;
    }
    let to_char = |p: usize| new_text[..p].chars().count();
    let (s, e) = (to_char(new_sel.start), to_char(new_sel.end));
    *query = new_text;
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(s),
            egui::text::CCursor::new(e),
        )));
    state.store(ctx, id);
}

/// **Format**: lay out the marked part of the editor, or the whole editor
/// when nothing is marked, and mark the formatted text afterwards.
pub fn format_in_editor(
    ctx: &egui::Context,
    pane: &mut crate::app::state::SqlPane,
    opts: &octa::sql::format::SqlFormatOptions,
    dialect: octa::sql::format::FormatDialect,
) {
    use super::text_ops::char_range_to_byte_range;
    let id = editor_id(pane.id);
    let query = &mut pane.query;
    let mut state = egui::TextEdit::load_state(ctx, id);
    let marked = state
        .as_ref()
        .and_then(|s| s.cursor.char_range())
        .map(|r| {
            let (a, b) = (r.primary.index.0, r.secondary.index.0);
            char_range_to_byte_range(query, a.min(b), a.max(b))
        })
        .filter(|r| r.start < r.end && !query[r.clone()].trim().is_empty())
        .unwrap_or(0..query.len());
    let formatted = octa::sql::format::format_sql(&query[marked.clone()], opts, dialect);
    if formatted == query[marked.clone()] {
        return;
    }
    query.replace_range(marked.clone(), &formatted);
    if let Some(state) = state.as_mut() {
        let start = query[..marked.start].chars().count();
        let end = start + formatted.chars().count();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(start),
                egui::text::CCursor::new(end),
            )));
        state.clone().store(ctx, id);
    }
}

/// SQL keywords offered by the autocomplete dropdown.
pub const SQL_KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP BY",
    "ORDER BY",
    "LIMIT",
    "OFFSET",
    "HAVING",
    "DISTINCT",
    "JOIN",
    "LEFT JOIN",
    "RIGHT JOIN",
    "INNER JOIN",
    "OUTER JOIN",
    "FULL JOIN",
    "CROSS JOIN",
    "ON",
    "AS",
    "AND",
    "OR",
    "NOT",
    "IS",
    "NULL",
    "IN",
    "BETWEEN",
    "LIKE",
    "ILIKE",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "UNION",
    "UNION ALL",
    "INTERSECT",
    "EXCEPT",
    "INSERT",
    "INTO",
    "VALUES",
    "UPDATE",
    "SET",
    "DELETE",
    "CREATE",
    "TABLE",
    "DROP",
    "ALTER",
    "ADD",
    "COLUMN",
    "WITH",
    "ASC",
    "DESC",
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "CAST",
    "COALESCE",
    "TRUE",
    "FALSE",
    "data",
];

/// Extract the partial token to the left of the cursor so it can be used as an
/// autocomplete prefix. Tokens are sequences of word characters; anything else
/// terminates the prefix.
pub fn current_prefix_at(text: &str, cursor_byte: usize) -> (usize, &str) {
    let mut cursor = cursor_byte.min(text.len());
    while !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    // Any letter, not only ASCII: a column called `Groesse` spelled with its
    // umlaut used to cut the prefix at the umlaut and match nothing.
    let start = text[..cursor]
        .char_indices()
        .rev()
        .take_while(|&(_, ch)| ch.is_alphanumeric() || ch == '_')
        .last()
        .map_or(cursor, |(i, _)| i);
    (start, &text[start..cursor])
}

/// Where a history entry ran, as `(short, full)` for the menu row and its
/// hover. The entry stores ids, so a renamed connection shows its new name and
/// a deleted one says so.
pub fn history_source_label(
    source: &str,
    db_connections: &[DbAttachEntry],
    cloud_connections: &[(String, String)],
) -> (String, String) {
    if let Some(id) = source.strip_prefix("db:") {
        let name = db_connections
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| octa::i18n::t("sql.history_deleted_conn"));
        return (name.clone(), name);
    }
    if let Some(rest) = source.strip_prefix("cloud:") {
        let (id, key) = rest.split_once(':').unwrap_or((rest, ""));
        let name = cloud_connections
            .iter()
            .find(|(cid, _)| cid == id)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| octa::i18n::t("sql.history_deleted_conn"));
        let file = key.rsplit('/').next().unwrap_or(key);
        return (format!("{name}: {file}"), format!("{name}: {key}"));
    }
    if let Some(path) = source.strip_prefix("file:") {
        let file = std::path::Path::new(path)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        return (file, path.to_string());
    }
    let scratch = octa::i18n::t("sql.history_scratch");
    (scratch.clone(), scratch)
}

/// Filter keywords + column names by a case-insensitive prefix match. Column
/// names win ties over keywords. Returns at most `max` entries.
pub fn collect_suggestions(prefix: &str, columns: &[String], max: usize) -> Vec<String> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let pfx = prefix.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for col in columns {
        if col.to_lowercase().starts_with(&pfx) {
            out.push(col.clone());
        }
    }
    for kw in SQL_KEYWORDS {
        if kw.to_lowercase().starts_with(&pfx) && !out.iter().any(|s| s == kw) {
            out.push((*kw).to_string());
        }
    }
    out.truncate(max);
    out
}

/// Lightweight view of one registered workspace table, passed to the SQL
/// panel renderer so it can list the tab's current workspace without
/// borrowing the `SqlWorkspace` directly (the workspace is consumed by
/// the parent loop's match on the returned `SqlAction`).
#[derive(Debug, Clone)]
pub struct WorkspaceRow {
    pub sql_name: String,
    pub origin: String,
    pub row_count: usize,
    /// `true` for the conventional `data` table; the panel renders it
    /// with a [refresh] affordance instead of a remove button.
    pub is_active: bool,
}

/// Lightweight view of one ATTACH-ed database, passed alongside
/// [`WorkspaceRow`]s for the workspace section.
#[derive(Debug, Clone)]
pub struct WorkspaceAttachment {
    pub alias: String,
    pub source: String,
    pub kind_label: &'static str,
    /// Native ATTACH versus fallback-loaded-as-tables.
    pub native: bool,
    pub table_count: usize,
    /// Per-schema groupings of the attached tables. Empty for fallback
    /// attachments. Pre-computed in `workspace_snapshot` so the renderer
    /// doesn't talk to the workspace directly.
    pub schemas: Vec<WorkspaceAttachmentSchema>,
}

/// Inner-table grouping inside a [`WorkspaceAttachment`].
#[derive(Debug, Clone)]
pub struct WorkspaceAttachmentSchema {
    pub schema: String,
    pub tables: Vec<WorkspaceAttachmentTable>,
}

/// Single attached-database table shown in the workspace tree.
#[derive(Debug, Clone)]
pub struct WorkspaceAttachmentTable {
    pub schema: String,
    pub table: String,
    pub row_count: Option<usize>,
}

/// Bundle of every per-call parameter that doesn't fit on the renderer's
/// natural argument list. Avoids the clippy lint on a 9-argument function
/// without losing the GUI / library separation (the workspace itself
/// stays in the panel, the renderer only sees this passive view).
pub struct SqlViewContext<'a> {
    pub autocomplete_enabled: bool,
    pub default_row_limit: usize,
    pub partial_rows: Option<(usize, usize)>,
    pub editor_font: octa::ui::settings::SqlEditorFont,
    pub workspace_tables: &'a [WorkspaceRow],
    pub workspace_attachments: &'a [WorkspaceAttachment],
    /// Currently selected inspector target. Drives both the highlight in the
    /// workspace tree on the left and the detail pane on the right.
    pub inspector_selection: Option<&'a crate::app::sql_panel::InspectorTarget>,
    /// Cached introspection result for the current selection. `None` while
    /// the panel waits for the first fetch; `Some(Ok)` or `Some(Err)`
    /// otherwise.
    pub inspector_entry: Option<&'a crate::app::state::InspectorCacheEntry>,
    /// Every query run in any SQL editor, most recent first.
    pub history: &'a [octa::sql::history::SqlHistoryEntry],
    /// Whether a "Run on server" query is currently in flight.
    pub server_running: bool,
    /// Saved live-database connections, for the attach menu.
    pub db_connections: Vec<DbAttachEntry>,
    /// Saved cloud connections as (id, name), for the cloud attach menu.
    pub cloud_connections: Vec<(String, String)>,
    /// Names the panel auto-registered the other open tabs under, and
    /// whether the note explaining that has still to be shown.
    pub auto_registered: &'a [String],
    pub show_auto_register_notice: bool,
    /// Whether at least one chat profile is configured, so the Ask box can be
    /// offered. Passed in because the view layer does not read settings.
    pub chat_profile_available: bool,
    /// Configured chat profiles as (id, name), for the Ask box's picker.
    pub ask_profiles: Vec<(String, String)>,
    /// More names for autocomplete that the workspace cannot see: the
    /// catalogs, schemas and tables of the live server a DB tab queries.
    pub extra_identifiers: &'a [String],
}

/// One saved live-database connection as the attach menu needs it.
#[derive(Clone)]
pub struct DbAttachEntry {
    pub id: String,
    pub name: String,
    /// `None` for an engine DuckDB attaches natively (Postgres, MySQL,
    /// Redshift): one click attaches the whole server and nothing is copied.
    /// `Some` for the import engines, where every attached table is fetched
    /// and the menu therefore drills `[catalog ->] schema -> table` so only
    /// the picked part is imported - see `SqlWorkspace::attach_db`.
    pub drill: Option<DrillMenu>,
}

/// What the attach menu needs to walk one import connection's tree.
#[derive(Clone)]
pub struct DrillMenu {
    /// Three-level engine (Trino, Snowflake, Databricks, BigQuery): the first
    /// level is catalogs, not schemas.
    pub has_catalogs: bool,
    /// Listings by node path below the connection root (`[]` = the root),
    /// mirrored from the sidebar's shared cache so the menu reuses the same
    /// background fetch instead of blocking the interface thread on a network
    /// call. A path that is absent has not been asked for yet.
    pub nodes: std::collections::HashMap<Vec<String>, NodeListing>,
}

/// State of one node's listing in [`DrillMenu::nodes`].
#[derive(Clone)]
pub enum NodeListing {
    Loading,
    Ready(Vec<String>),
    Failed(String),
}

/// Row-counter text for a result grid.
///
/// `total` is the exact size of the whole result when the statement was paged,
/// and `rows` what has been fetched so far - so a paged result reads
/// "1,000 of 8,432,109 result rows" and says what it is instead of reporting
/// the page as if it were the answer. Without a total, a result sitting
/// exactly on the initial-load cap is truncation and says so.
///
/// `took_ms` appends how long the query ran. It is spelled in `ms` / `s`
/// rather than a translated phrase: both are SI symbols, so the line needs no
/// thirty-second locale key to say "in".
fn result_rows_label(rows: usize, total: Option<usize>, took_ms: Option<u64>) -> String {
    let counted = match total {
        Some(t) if t > rows => format!(
            "{} / {} {}",
            format_number(rows),
            format_number(t),
            octa::i18n::t("sql.result_rows")
        ),
        Some(t) => format!("{} {}", format_number(t), octa::i18n::t("sql.result_rows")),
        None if rows >= octa::formats::initial_load_rows() => format!(
            "{} {}",
            format_number(rows),
            octa::i18n::t("sql.result_rows_capped")
        ),
        None => format!(
            "{} {}",
            format_number(rows),
            octa::i18n::t("sql.result_rows")
        ),
    };
    match took_ms {
        Some(ms) => format!("{counted} ({})", format_duration(ms)),
        None => counted,
    }
}

/// One pane of the SQL panel's splitter, top to bottom.
#[derive(Clone, Copy)]
enum Pane {
    WorkspaceTree,
    Inspector,
    Editor,
    Result,
}

/// Width for the Ask box: the row it now has to itself, less the Ask button
/// and (when there is a choice to make) the profile combo beside it.
///
/// Clamped at both ends. A panel docked narrow leaves less than the controls
/// need, and the subtraction goes negative there; a panel across a wide screen
/// would otherwise hand a single question a 2000px box.
fn ask_box_width(available: f32, has_profile_combo: bool) -> f32 {
    let reserved = if has_profile_combo { 230.0 } else { 96.0 };
    (available - reserved).clamp(160.0, 640.0)
}

/// A query duration a person can read at a glance: milliseconds while they
/// stay small, seconds once they do not.
fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

/// Render a split-pane SQL editor (top) and result table (bottom).
/// The current tab's table is exposed in queries as `data`.
/// `partial_rows` carries `(loaded, total)` when the table isn't fully loaded.
pub fn render_sql_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    ctx_args: SqlViewContext<'_>,
) -> SqlAction {
    let SqlViewContext {
        autocomplete_enabled,
        default_row_limit,
        partial_rows,
        editor_font,
        workspace_tables,
        workspace_attachments,
        inspector_selection,
        inspector_entry,
        history,
        server_running,
        db_connections,
        cloud_connections,
        auto_registered,
        show_auto_register_notice,
        chat_profile_available,
        ask_profiles,
        extra_identifiers,
    } = ctx_args;
    let mut action = SqlAction::default();
    // The focused editor is the active one: Run, Format, History, Export and
    // the shortcuts all act on it.
    let focused = ui.ctx().memory(|m| m.focused());
    if let Some(i) =
        (0..tab.sql_pane_count()).find(|&i| focused == Some(editor_id(tab.sql_pane_id(i))))
    {
        tab.activate_sql_pane(i);
    }
    let editor_id = editor_id(tab.sql.id);

    // Calm hover feedback for the whole panel: several themes paint hovered
    // widgets 1-3px larger (`widgets.hovered.expansion`), which in this dense
    // grid of cells, rows and small buttons reads as everything twitching
    // under the pointer. Zeroing it here is local to the SQL panel; hover
    // colours still change, nothing grows.
    {
        let style = ui.style_mut();
        style.visuals.widgets.hovered.expansion = 0.0;
        style.visuals.widgets.active.expansion = 0.0;
    }

    octa::ui::control_row::control_row(ui, |ui| {
        ui.label(egui::RichText::new(octa::i18n::t("sql.query_against_data")).strong());
        ui.add_space(8.0);
        // Where the query runs: the local DuckDB workspace, or straight on any
        // saved connection's server, where every table there is queryable by
        // its real name without attaching anything.
        ui.label(octa::i18n::t("sql.run_on"));
        let local = octa::i18n::t("sql.run_local");
        let selected = tab
            .sql_target
            .as_deref()
            .map(|id| {
                db_connections
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| octa::i18n::t("sql.history_deleted_conn"))
            })
            .unwrap_or_else(|| local.clone());
        let hint = match tab.sql_target {
            Some(_) => octa::i18n::t("sql.run_on_server_hint"),
            None => octa::i18n::t("sql.run_local_hint"),
        };
        ui.add_enabled_ui(!server_running, |ui| {
            egui::ComboBox::from_id_salt("sql_run_on")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut tab.sql_target, None, local)
                        .on_hover_text(octa::i18n::t("sql.run_local_hint"));
                    for c in &db_connections {
                        ui.selectable_value(&mut tab.sql_target, Some(c.id.clone()), &c.name)
                            .on_hover_text(octa::i18n::t("sql.run_on_server_hint"));
                    }
                })
                .response
                .on_hover_text(format!("{}\n\n{hint}", octa::i18n::t("sql.run_on_hint")))
                .on_disabled_hover_text(octa::i18n::t("sql.run_on_busy"));
        });
        ui.add_space(4.0);
        if server_running {
            ui.add(egui::Spinner::new().size(12.0));
            if ui.button(octa::i18n::t("common.cancel")).clicked() {
                action.cancel_server = true;
            }
        } else if ui
            .button(octa::i18n::t("sql.run"))
            .on_hover_text(octa::i18n::t("sql.run_hint"))
            .clicked()
        {
            action.run = true;
        }
        if ui.button(octa::i18n::t("sql.clear_result")).clicked() {
            action.clear = true;
        }
        if ui
            .button(octa::i18n::t("sql.format"))
            .on_hover_text(octa::i18n::t("sql.format_hint"))
            .clicked()
        {
            action.format = true;
        }
        if ui
            .button("+")
            .on_hover_text(octa::i18n::t("sql.add_editor_hint"))
            .clicked()
        {
            action.add_pane = true;
        }

        // History: every query run in any SQL editor, kept between sessions,
        // one list. Each row says where it ran and what it cost; hovering
        // shows the whole statement. Picking one only puts the text in the
        // editor, the target stays as it is.
        if !history.is_empty() {
            ui.menu_button(octa::i18n::t("sql.history"), |ui| {
                ui.set_min_width(360.0);
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        for h in history {
                            // One-line preview, counted in chars, not bytes:
                            // `&preview[..57]` panicked on any query holding a
                            // multi-byte character.
                            let flat = h.query.split_whitespace().collect::<Vec<_>>().join(" ");
                            let preview = if flat.chars().count() > 60 {
                                format!("{}...", flat.chars().take(57).collect::<String>())
                            } else {
                                flat
                            };
                            let (short, full) = history_source_label(
                                &h.source,
                                &db_connections,
                                &cloud_connections,
                            );
                            let cost = format!(
                                "{short} - {} {} - {} ms",
                                octa::ui::status_bar::format_number(h.rows),
                                octa::i18n::t("sql.history_rows"),
                                octa::ui::status_bar::format_number(h.duration_ms as usize)
                            );
                            let when = chrono::DateTime::from_timestamp(h.at_unix as i64, 0)
                                .map(|t| {
                                    t.with_timezone(&chrono::Local)
                                        .format("%Y-%m-%d %H:%M")
                                        .to_string()
                                })
                                .unwrap_or_default();
                            ui.horizontal(|ui| {
                                let resp = ui.button(preview).on_hover_ui(|ui| {
                                    ui.label(egui::RichText::new(format!("{full}  {when}")).weak());
                                    ui.separator();
                                    ui.label(egui::RichText::new(&h.query).monospace());
                                });
                                if resp.clicked() {
                                    action.recall_query = Some(h.query.clone());
                                    ui.close();
                                }
                                // `weak` rather than a theme colour: this view
                                // has no ThemeColors in scope, and weak text is
                                // exactly the "secondary detail" role here.
                                ui.label(egui::RichText::new(cost).size(11.0).weak());
                            });
                        }
                    });
                ui.separator();
                if ui
                    .button(octa::i18n::t("sql.history_clear"))
                    .on_hover_text(octa::i18n::t("sql.history_clear_hint"))
                    .clicked()
                {
                    action.clear_history = true;
                    ui.close();
                }
            })
            .response
            .on_hover_text(octa::i18n::t("sql.history_hint"));
        }

        // Snippets: opens the persistent named-query manager window.
        if ui
            .button(octa::i18n::t("sql.snippets"))
            .on_hover_text(octa::i18n::t("sql.snippets_hint"))
            .clicked()
        {
            action.open_snippets_window = true;
        }

        let has_result = tab.sql.result.as_ref().is_some_and(|t| t.col_count() > 0);
        ui.add_enabled_ui(has_result, |ui| {
            if ui
                .button(octa::i18n::t("sql.export"))
                .on_hover_text(octa::i18n::t("sql.export_hint"))
                .clicked()
            {
                action.export = true;
            }
            if ui
                .button(octa::i18n::t("sql.write_to_db"))
                .on_hover_text(octa::i18n::t("sql.write_to_db_hint"))
                .clicked()
            {
                action.open_write_back = true;
            }
            if ui
                .button(octa::i18n::t("sql.result_to_tab"))
                .on_hover_text(octa::i18n::t("sql.result_to_tab_hint"))
                .on_disabled_hover_text(octa::i18n::t("sql.result_to_tab_hint"))
                .clicked()
            {
                action.result_to_tab = true;
            }
        });
        if let Some(rows) = tab.sql.result.as_ref().map(|t| t.row_count()) {
            ui.add_space(12.0);
            ui.label(result_rows_label(
                rows,
                tab.sql.result_total,
                tab.sql.last_duration_ms,
            ));
        }
        // Close (×) button on the right - flips `sql_panel_open` to false.
        // The Analyse dropdown is two clicks away, so without an in-panel
        // close the user has to fiddle to dismiss it.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(egui::RichText::new("\u{00d7}").size(16.0).strong())
                .on_hover_text(octa::i18n::t("sql.close_hint"))
                .clicked()
            {
                action.close = true;
            }
            let (label, hint) = if tab.sql_maximised {
                ("sql.restore", "sql.restore_hint")
            } else {
                ("sql.maximise", "sql.maximise_hint")
            };
            if ui
                .button(octa::i18n::t(label))
                .on_hover_text(octa::i18n::t(hint))
                .clicked()
            {
                action.toggle_maximise = true;
            }
        });
    });

    // Ask: plain language in, one SELECT out, into the editor at the
    // cursor. Never runs. Disabled with a reason rather than hidden, so
    // the control explains itself.
    // Anything the prompt can describe is enough. The old test was "does
    // the tab hold a table", which switched Ask off on exactly the tab the
    // panel exists to support: an empty one whose workspace has a server
    // ATTACHed. Those tables are describable and queryable; the tab having
    // no file of its own says nothing about that.
    let has_schema = tab.table.col_count() > 0
        || !workspace_tables.is_empty()
        || !workspace_attachments.is_empty();
    // Pointed at a connection with no table of this tab on it: there is
    // nothing to describe to the model, and guessing would write SQL for the
    // wrong database.
    let foreign_target =
        tab.sql_target.is_some() && crate::app::sql_panel::server_origin(tab).is_none();
    let ask_enabled = chat_profile_available && has_schema && !foreign_target;
    let ask_reason = if !chat_profile_available {
        octa::i18n::t("sql.ask_needs_profile")
    } else if foreign_target {
        octa::i18n::t("sql.ask_needs_table")
    } else if !has_schema {
        octa::i18n::t("sql.ask_no_columns")
    } else {
        format!(
            "{}\n\n{}",
            octa::i18n::t("sql.ask_hint"),
            octa::i18n::t("sql.ask_keys")
        )
    };
    // Its own row, and the box goes in first, for one reason: a horizontal
    // layout centres each widget against the row height *known when that
    // widget is added*, and the row grows the moment something taller lands
    // in it (`Placer::advance_after_rects` -> `expand_to_include_rect`). The
    // Ask box is a multiline TextEdit and taller than a button, so while it
    // sat in the toolbar everything after it - Ask, the model combo, Export,
    // Write to DB, the row count - centred against the grown row and sat
    // visibly lower than Run, Clear, History and Snippets before it. Nothing
    // is added ahead of the box here, so there is nothing left to stagger,
    // and the box may grow as the question wraps without moving anything.
    ui.horizontal(|ui| {
        ui.spacing_mut().interact_size.y = control_height(ui);
        ui.add_enabled_ui(ask_enabled, |ui| {
            // Multiline, one row tall to start: a question longer than the box
            // used to scroll sideways behind itself, unreadable while typing
            // it. It grows downwards as the text wraps. Width comes from the row
            // now that it has one to itself, less what the controls beside it
            // need.
            let box_width = ask_box_width(ui.available_width(), ask_profiles.len() > 1);
            let pad = ui.spacing().button_padding.y.round() as i8;
            let box_resp = ui
                .add(
                    egui::TextEdit::multiline(&mut tab.sql_ask_input)
                        .desired_rows(1)
                        .margin(egui::Margin::symmetric(4, pad))
                        .desired_width(box_width)
                        .hint_text(octa::i18n::t("sql.ask_placeholder")),
                )
                .on_hover_text(ask_reason.clone())
                .on_disabled_hover_text(ask_reason.clone());
            // Enter sends, Shift+Enter breaks the line - the chord the chat
            // panel already uses. A multiline box keeps focus on Enter, so
            // the old `lost_focus()` test would never fire again.
            let submitted = box_resp.has_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
            let clicked = ui
                .button(octa::i18n::t("sql.ask"))
                .on_hover_text(ask_reason.clone())
                .on_disabled_hover_text(ask_reason)
                .clicked();
            // Which assistant answers, chosen per tab. The same control the
            // search bar's Ask already has; without it this box could only
            // ever use whatever the chat panel happened to be set to. One
            // configured profile means no choice to make, so no combo.
            if ask_profiles.len() > 1 {
                let selected = ask_profiles
                    .iter()
                    .find(|(id, _)| id == &tab.sql_ask_profile)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("sql_ask_profile")
                    .width(130.0)
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (id, name) in &ask_profiles {
                            ui.selectable_value(&mut tab.sql_ask_profile, id.clone(), name);
                        }
                    })
                    .response
                    .on_hover_text(octa::i18n::t("sql.ask_profile_hint"));
            }
            if submitted || clicked {
                // The Enter that sent this reached the box first and left its
                // newline behind. Take it back out: a question the assistant
                // could not answer stays in the box, and it should stay
                // exactly as it was typed rather than a line taller.
                let question = tab.sql_ask_input.trim().to_string();
                if !question.is_empty() {
                    tab.sql_ask_input.clone_from(&question);
                    action.ask = Some(question);
                }
            }
        });
    });

    // One-time note naming what the panel registered on the user's behalf.
    // It explains a behaviour nobody asked for, so it says what happened, what
    // the tables are called, and where to switch it off.
    if show_auto_register_notice && !auto_registered.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} {}",
                    octa::i18n::t("sql.auto_reg_note"),
                    octa::ui::message::elide_list(auto_registered, 6),
                ))
                .small()
                .color(ui.visuals().weak_text_color()),
            )
            .on_hover_text(auto_registered.join(", "));
            if ui
                .small_button(octa::i18n::t("view.dismiss"))
                .on_hover_text(octa::i18n::t("sql.auto_reg_note_hint"))
                .clicked()
            {
                action.dismiss_auto_register_notice = true;
            }
        });
        ui.add_space(2.0);
    }

    // A tab from a live connection queries the server directly, where its
    // sibling tables are already joinable by their real names. Say so: the
    // alternative people reach for is copying them into DuckDB one by one.
    if let Some(conn) = tab
        .sql_target
        .as_deref()
        .and_then(|id| db_connections.iter().find(|c| c.id == id))
        .map(|c| &c.name)
    {
        ui.label(
            egui::RichText::new(octa::i18n::t("sql.server_tables_note").replace("{conn}", conn))
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(2.0);
    }

    ui.add_space(4.0);

    let workspace_data = WorkspaceData {
        tables: workspace_tables,
        attachments: workspace_attachments,
        db_connections: &db_connections,
        cloud_connections: &cloud_connections,
    };
    let workspace_open = render_workspace_section(ui, tab, &workspace_data);
    ui.add_space(4.0);

    // --- Compute autocomplete state BEFORE rendering the TextEdit so we can
    // intercept arrow / Enter / Tab / Escape keys while the popup is visible.
    let editor_focused = ui.ctx().memory(|m| m.focused() == Some(editor_id));
    let mut suggestions: Vec<String> = Vec::new();
    let mut prefix_start = 0usize;
    let mut prefix_len = 0usize;
    if autocomplete_enabled && editor_focused {
        // Suggestions draw from every identifier the SQL workspace can see -
        // not just the active `data` table's columns. The workspace's
        // `information_schema` query covers registered table names,
        // attachment aliases, attached-database tables, and every column of
        // every visible table; we merge the active tab's column list on top
        // so freshly-added columns surface even before the user clicks the
        // workspace [refresh] button.
        let mut idents: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
        if let Some(ws) = tab.sql_workspace.as_ref() {
            idents.extend(ws.collect_autocomplete_identifiers());
        }
        idents.extend_from_slice(extra_identifiers);
        idents.sort();
        idents.dedup();
        let columns = idents;
        let cursor_byte = egui::TextEdit::load_state(ui.ctx(), editor_id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let char_idx = r.primary.index.0;
                tab.sql
                    .query
                    .char_indices()
                    .nth(char_idx)
                    .map(|(i, _)| i)
                    .unwrap_or_else(|| tab.sql.query.len())
            })
            .unwrap_or(tab.sql.query.len());
        let (pstart, pstr) = current_prefix_at(&tab.sql.query, cursor_byte);
        prefix_start = pstart;
        prefix_len = pstr.len();
        if !pstr.is_empty() {
            suggestions = collect_suggestions(pstr, &columns, 50);
        }
    }

    // Clamp selection against the live list.
    if !suggestions.is_empty() {
        if tab.sql.ac_selected >= suggestions.len() {
            tab.sql.ac_selected = 0;
        }
    } else {
        tab.sql.ac_selected = 0;
    }

    // Consume the popup-specific keys *only while the popup is visible*
    // (`popup_active`): Up/Down move the selection, Enter or Tab accepts the
    // highlighted suggestion, Escape dismisses. Because consumption is gated on
    // `popup_active`, these keys keep their normal editor behaviour whenever the
    // popup is closed - Enter inserts a newline, arrows move the caret. The
    // popup only opens after the user types (`resp.changed()` sets
    // `sql_ac_visible`) and closes on Escape, so plain typing never loses keys.
    let popup_active = editor_focused && tab.sql.ac_visible && !suggestions.is_empty();
    let mut apply_suggestion: Option<String> = None;
    // Moved by the keyboard this frame: the popup scrolls the pick into view.
    let mut ac_keyed = false;
    if popup_active {
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                tab.sql.ac_selected = (tab.sql.ac_selected + 1) % suggestions.len();
                ac_keyed = true;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                ac_keyed = true;
                tab.sql.ac_selected = if tab.sql.ac_selected == 0 {
                    suggestions.len() - 1
                } else {
                    tab.sql.ac_selected - 1
                };
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)
            {
                apply_suggestion = suggestions.get(tab.sql.ac_selected).cloned();
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                tab.sql.ac_visible = false;
            }
        });
    }

    let mut editor_response: Option<egui::Response> = None;
    // A click anywhere in a pane's column makes it the active pane, applied
    // after everything below has worked on the pane that was active.
    let mut clicked_pane: Option<usize> = None;

    let mut load_more_rows = None;
    let mut cancel_server = false;
    let render_result_area = |ui: &mut egui::Ui,
                              tab: &mut TabState,
                              load_more_rows: &mut Option<u64>,
                              cancel_server: &mut bool| {
        // The splitter owns the height; the body just fills what it was given.
        ui.set_min_height(ui.available_height());
        // Running: say so, big, in place of the last result. Leaving the old
        // grid up read as the answer to the new query.
        if let Some(since) = tab.sql.running_since {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() / 2.0 - 40.0).max(8.0));
                ui.add(egui::Spinner::new().size(32.0));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        octa::i18n::t("sql.running"),
                        format_duration(since.elapsed().as_millis() as u64)
                    ))
                    .heading(),
                );
                // Only a server query runs off the interface thread; a local
                // one has the window until it returns, so no button there.
                if server_running
                    && tab.sql_target.is_some()
                    && ui
                        .button(octa::i18n::t("common.cancel"))
                        .on_hover_text(octa::i18n::t("sql.cancel_hint"))
                        .clicked()
                {
                    *cancel_server = true;
                }
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        if let Some((loaded, total)) = partial_rows {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "\u{26a0} Result based on {} of {} rows currently loaded.",
                        format_number(loaded),
                        format_number(total),
                    ))
                    .small()
                    .color(egui::Color32::from_rgb(200, 160, 50)),
                );
            });
            ui.add_space(2.0);
        }
        if let Some(err) = &tab.sql.error {
            ui.colored_label(
                egui::Color32::from_rgb(220, 80, 80),
                format!("Error: {err}"),
            );
            ui.add_space(4.0);
        }
        if let Some(result) = tab.sql.result.as_ref() {
            // Row counter directly at the result, so the count is always in
            // view without adding COUNT(*) to the query or exporting.
            ui.label(
                egui::RichText::new(result_rows_label(
                    result.row_count(),
                    tab.sql.result_total,
                    tab.sql.last_duration_ms,
                ))
                .small()
                .color(ui.visuals().weak_text_color()),
            );
            // Disjoint field borrows: the result table (read) + its selection
            // (write).
            if render_result_table(ui, result, &mut tab.sql.result_sel, tab.sql.result_total) {
                *load_more_rows = Some(tab.sql.id);
            }
        } else if tab.sql.error.is_none() {
            ui.label(egui::RichText::new(octa::i18n::t("sql.run_to_see")).weak());
        }
    };

    // Every pane the panel shows, top to bottom, sharing what is left of it.
    // One splitter rather than a chain of nested panels: the panes add up to
    // the space exactly, so a drag moves the boundary it grabbed and nothing
    // else, and each pane is clipped to its slot, so a tall editor or a long
    // result can never be drawn over its neighbour.
    let mut panes = Vec::with_capacity(4);
    if workspace_open {
        panes.push(Pane::WorkspaceTree);
        panes.push(Pane::Inspector);
    }
    panes.push(Pane::Editor);
    panes.push(Pane::Result);
    let rects = splitter::vertical_splitter(ui, ui.id().with("sql_panes"), panes.len());

    for (pane, rect) in panes.into_iter().zip(rects) {
        splitter::pane(ui, rect, |ui| match pane {
            Pane::WorkspaceTree => workspace::render_workspace_list(
                ui,
                tab,
                &workspace_data,
                inspector_selection,
                &mut action,
            ),
            Pane::Inspector => workspace::render_inspector_pane(
                ui,
                &workspace_data,
                inspector_selection,
                inspector_entry,
                &mut action,
            ),
            Pane::Editor => {
                editor_response = each_pane(ui, tab, &mut clicked_pane, |ui, tab, i, n| {
                    if n > 1 {
                        octa::ui::control_row::control_row(ui, |ui| {
                            ui.label(
                                egui::RichText::new(
                                    octa::i18n::t("sql.editor_n")
                                        .replace("{n}", &(i + 1).to_string()),
                                )
                                .small()
                                .weak(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("\u{00d7}")
                                        .on_hover_text(octa::i18n::t("sql.close_editor_hint"))
                                        .clicked()
                                    {
                                        action.close_pane = Some(i);
                                    }
                                },
                            );
                        });
                    }
                    draw_sql_editor(
                        ui,
                        tab,
                        self::editor_id(tab.sql.id),
                        default_row_limit,
                        &mut action,
                        editor_font,
                    )
                });
            }
            Pane::Result => {
                each_pane(ui, tab, &mut clicked_pane, |ui, tab, _, _| {
                    render_result_area(ui, tab, &mut load_more_rows, &mut cancel_server)
                });
            }
        });
    }
    let editor_response = editor_response.expect("the active editor pane always renders");

    // Right-click context menu on the SQL editor: selection-aware Copy +
    // whole-buffer Copy All.
    {
        let buffer = tab.sql.query.clone();
        editor_response.clone().context_menu(|ui| {
            let selection = super::text_ops::selected_text(ui.ctx(), editor_id, &buffer);
            let copy_label = if selection.is_some() {
                octa::i18n::t("header.copy")
            } else {
                octa::i18n::t("sql.copy_no_selection")
            };
            let copy_btn = ui.add_enabled(selection.is_some(), egui::Button::new(copy_label));
            if copy_btn.clicked() {
                if let Some(s) = selection {
                    ui.ctx().copy_text(s);
                }
                ui.close();
            }
            if ui.button(octa::i18n::t("view.copy_all")).clicked() {
                ui.ctx().copy_text(buffer.clone());
                ui.close();
            }
        });
    }

    // Apply the chosen suggestion: replace the current prefix, move the caret
    // to the end of the inserted text, refocus the editor.
    if let Some(sugg) = apply_suggestion {
        let end = prefix_start + prefix_len;
        if end <= tab.sql.query.len() {
            tab.sql.query.replace_range(prefix_start..end, &sugg);
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), editor_id) {
                let new_char_idx = tab.sql.query[..prefix_start + sugg.len()].chars().count();
                let ccursor = egui::text::CCursor::new(new_char_idx);
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                state.store(ui.ctx(), editor_id);
            }
            editor_response.request_focus();
        }
    }

    // --- Autocomplete popup ---
    if popup_active {
        let popup_id = ui.make_persistent_id("sql_autocomplete_popup");
        egui::Popup::from_response(&editor_response)
            .id(popup_id)
            .open(true)
            .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
            .show(|ui| {
                ui.set_min_width(220.0);
                // Force a high-contrast text color on the selected chip so the
                // variable name stays readable against the translucent selection
                // tint that some dark themes use for `selection.bg_fill`. egui's
                // default selectable_label inherits `widgets.inactive.fg_stroke`
                // for selected items, which produced barely-visible text in
                // Dark / Nord / Dracula / Gruvbox / DeepSea / Gentleman.
                let strong_color = if ui.visuals().dark_mode {
                    egui::Color32::WHITE
                } else {
                    ui.visuals().strong_text_color()
                };
                // Scrolls: a wide table can offer far more matching columns
                // than fit, and a cap small enough to fit hid real columns.
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for (idx, s) in suggestions.iter().enumerate() {
                            let selected = idx == tab.sql.ac_selected;
                            let label = if selected {
                                egui::RichText::new(s).color(strong_color).strong()
                            } else {
                                egui::RichText::new(s)
                            };
                            let resp = ui.selectable_label(selected, label);
                            if selected && ac_keyed {
                                resp.scroll_to_me(None);
                            }
                            if resp.clicked() {
                                apply_suggestion_later(tab, prefix_start, prefix_len, s, ui.ctx());
                                editor_response.request_focus();
                            }
                            if resp.hovered() {
                                tab.sql.ac_selected = idx;
                            }
                        }
                    });
            });
    }

    // Ctrl+C on a result selection. The editor only consumes the Copy event
    // while it is focused; clicking a result cell moved focus to that cell, so
    // here the event survives - copy the selection and consume it. When the
    // editor is focused instead, it handled its own copy and there's nothing
    // to do.
    let editor_focused = ui.ctx().memory(|m| m.focused()) == Some(editor_id);
    let text_marked = octa::ui::text_selection::has_active_selection(ui.ctx());
    if !editor_focused
        && !text_marked
        && !tab.sql.result_sel.is_empty()
        && let Some(result) = &tab.sql.result
    {
        let want_copy = ui.input_mut(|i| {
            let had = i
                .events
                .iter()
                .any(|e| matches!(e, egui::Event::Copy | egui::Event::Cut));
            i.events
                .retain(|e| !matches!(e, egui::Event::Copy | egui::Event::Cut));
            had
        });
        if want_copy {
            // A single cell goes without its trailing newline, as before, so
            // pasting it into a form field does not add a line.
            let tsv = selection_to_tsv(result, &tab.sql.result_sel);
            ui.ctx()
                .copy_text(tsv.strip_suffix('\n').unwrap_or(&tsv).to_string());
        }
    }

    if let Some(i) = clicked_pane {
        tab.activate_sql_pane(i);
    }
    action.load_more_rows = load_more_rows;
    action.cancel_server |= cancel_server;
    action
}

/// Draw `body` once per editor pane, side by side, with that pane swapped
/// into `tab.sql` so the single-editor code draws it unchanged. Returns what
/// `body` gave for the active pane. A primary click inside a column is
/// reported in `clicked`; the active pane gets an outline when there is more
/// than one.
fn each_pane<R>(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    clicked: &mut Option<usize>,
    mut body: impl FnMut(&mut egui::Ui, &mut TabState, usize, usize) -> R,
) -> Option<R> {
    let n = tab.sql_pane_count();
    let active = tab.sql_active_pane;
    let accent = ui.visuals().selection.stroke.color;
    let mut out = None;
    ui.columns(n, |cols| {
        for (i, col) in cols.iter_mut().enumerate() {
            tab.activate_sql_pane(i);
            let r = col.push_id(tab.sql.id, |ui| body(ui, tab, i, n)).inner;
            let rect = col.max_rect().intersect(col.clip_rect());
            if n > 1 && i == active {
                col.painter().rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.0, accent),
                    egui::StrokeKind::Inside,
                );
            }
            if col.input(|inp| inp.pointer.primary_clicked()) && col.rect_contains_pointer(rect) {
                *clicked = Some(i);
            }
            if i == active {
                out = Some(r);
            }
        }
    });
    tab.activate_sql_pane(active);
    out
}

mod editor;
mod result;
mod splitter;
mod workspace;

use editor::{apply_suggestion_later, draw_sql_editor};
use result::render_result_table;
pub use result::{SqlResultSelection, selection_to_tsv};
use workspace::{WorkspaceData, render_workspace_section};

#[cfg(test)]
#[path = "../sql_tests.rs"]
mod tests;
