//! Settings dialog UI rendering. The full `impl SettingsDialog` lives here;
//! the struct definition + supporting `AppSettings` plus enums stay in
//! [`super`]. Split out purely for navigability - no behaviour change.

use egui;

use super::*;
use crate::ui::dialog_chrome::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};
use crate::ui::shortcuts::ShortcutAction;

mod appearance_section;
mod chat_section;
mod cloud_section;
mod db_section;
mod diagnostics_section;
mod directory_tree_section;
mod files_section;
mod format_section;
mod map_section;
mod mcp_section;
mod performance_section;
mod search_editor_section;
mod shortcuts_grid;
mod sql_section;
mod summary_section;
mod table_section;
mod updates_section;
mod window_section;

// ---------------------------------------------------------------------------
// Settings-dialog state.
//
// Declared here in `dialog/mod.rs` rather than in a sibling file: the fields
// below are private, and Rust makes a private field visible only inside the
// declaring module and its descendants. The per-section renderers are children
// of `dialog`, so they can see these; a sibling `dialog::state` module could
// not, which is 555 compile errors' worth of difference.
// ---------------------------------------------------------------------------

/// A chat connection test the dialog wants run. The provider adapters live in
/// the binary and this dialog in the library, so the request crosses that line
/// as plain data: the app drains it once per frame, runs one tiny turn on a
/// worker thread, and writes the outcome into `slot`.
pub struct ChatTestRequest {
    /// The profile as the form currently describes it (unsaved edits included).
    pub profile: chat_profiles::ChatModelProfile,
    /// Already-resolved key; empty for a keyless provider.
    pub api_key: String,
    /// Global base URL to use when the profile carries none (Ollama /
    /// OpenAI-compatible only). Empty otherwise.
    pub fallback_base_url: String,
    pub slot: ChatTestSlot,
    pub ctx: egui::Context,
}

/// A secret the dialog has just deleted, for the app to delete from the live
/// settings as well.
///
/// Clearing a key is a real, immediate side effect: the keyring entry is gone
/// the moment the button is clicked. The plaintext fallback, though, was only
/// removed from the draft - so closing Settings with the `x` (which is Cancel)
/// left the key sitting in `settings.toml` after the UI had said "cleared".
/// The string is the connection / profile id the secret is filed under.
#[derive(Debug, Clone)]
pub enum SecretPurge {
    Chat(String),
    Cloud(String),
    Db(String),
}

/// A recorded combo that another action already owns, plus who owns it, so the
/// grid can offer to move the binding instead of only refusing it.
#[derive(Debug, Clone, Copy)]
pub struct ShortcutTakeover {
    /// The action being recorded, which would gain the combo.
    pub action: ShortcutAction,
    /// The combo the user pressed.
    pub combo: crate::ui::shortcuts::KeyCombo,
    /// The action that holds it today, and would be left unbound.
    pub previous: ShortcutAction,
}

/// Transient state for the settings dialog.
#[derive(Default)]
pub struct SettingsDialog {
    pub open: bool,
    /// Working copy - committed on Apply/OK.
    pub draft: AppSettings,
    /// Snapshot of the settings the draft was seeded from. The app keeps
    /// running behind this window, so on Apply the two are compared to tell
    /// "the user changed this here" from "another surface changed it since"
    /// - see [`SettingsDialog::carry_external_edits`].
    seed: AppSettings,
    /// Whether the icon changed (needs texture + window icon refresh).
    pub icon_changed: bool,
    /// Whether font size changed (needs style reapply).
    pub font_changed: bool,
    /// Whether theme changed.
    pub theme_changed: bool,
    /// Buffer backing the SQL row-limit text input. Parsed into the draft
    /// on Apply so the user can type freely without drag widgets fighting them.
    sql_row_limit_buf: String,
    /// Text buffer behind the default Parquet row-group size. Empty means
    /// "leave it to the writer".
    write_row_group_buf: String,
    /// Buffer backing the syntax-highlight size text input. Holds the value
    /// in whichever unit `syntax_highlight_size_unit` currently picks, with
    /// comma thousand separators so it matches Octa's display conventions.
    /// Parsed on Apply.
    syntax_highlight_max_bytes_buf: String,
    /// Display unit for the syntax-highlight size input. Not persisted -
    /// reset each time the dialog opens.
    syntax_highlight_size_unit: SizeUnit,
    /// Buffer and display unit for the large-file size threshold, same shape
    /// as the syntax-highlight pair above. Parsed on Apply.
    large_file_min_bytes_buf: String,
    large_file_size_unit: SizeUnit,
    /// Buffer backing the initial-load-rows text input. Holds a comma-
    /// separated integer (e.g. "1,000,000"). Parsed on Apply.
    initial_load_rows_buf: String,
    /// Buffer backing the live-database page-size input. Comma-separated
    /// integer, parsed on Apply.
    db_page_rows_buf: String,
    /// Buffer backing the raw-view size cap input, in whole MB (the stored
    /// value is bytes; converted on open / Apply). Parsed on Apply.
    raw_view_max_mb_buf: String,
    /// Buffer backing the transparent-decompression size cap input, in whole
    /// MB (the stored value is bytes; converted on open / Apply).
    max_decompressed_mb_buf: String,
    /// Buffer backing the folder-union file cap input. Comma-tolerant integer;
    /// ignored while `AppSettings.folder_union_max_files_unlimited` is ticked.
    folder_union_max_files_buf: String,
    /// Buffer backing the user-extensible "treat as text" extensions input.
    /// Comma- or space-separated; canonicalised on Apply (lowercased,
    /// leading dot stripped). Parsed on Apply.
    text_mode_extensions_buf: String,
    /// Buffer backing the MCP default-row-limit text input. Comma-separated
    /// integer; ignored when `mcp_unlimited_rows` is checked.
    mcp_row_limit_buf: String,
    /// When true, the MCP server returns every row by default (the row
    /// limit input is greyed out). Mirrors `AppSettings.mcp_default_row_limit ==
    /// None`. Toggling on Apply writes `None`; toggling off writes
    /// `Some(parse(mcp_row_limit_buf))`.
    mcp_unlimited_rows: bool,
    /// Buffer backing the MCP default cell-byte cap input. Comma-separated
    /// integer; `0` means unlimited (same as the field semantic).
    mcp_cell_bytes_buf: String,
    /// Buffer backing the Multi-search file-size cap input. Comma-separated
    /// integer in megabytes; parsed back into `grep_max_file_size_mb` on Apply.
    /// Lives here (not on the field directly) so hover over the input doesn't
    /// flash the drag-resize cursor egui's `DragValue` always renders.
    grep_max_file_size_buf: String,
    /// Buffer backing the chart `max_points` input. Same pattern as
    /// `initial_load_rows_buf` so the user can paste "1,000,000" without
    /// fighting commas.
    chart_max_points_buf: String,
    status_message_secs_buf: String,
    /// Buffer backing the chart `max_categories` input.
    chart_max_categories_buf: String,
    /// Buffer backing the table-picker visible-rows input. Same comma-tolerant
    /// pattern as the other numeric inputs.
    table_picker_visible_rows_buf: String,
    /// Buffer backing the Excel max-auto-sheets input.
    excel_max_auto_sheets_buf: String,
    /// Buffer backing the search-history-size input (text, not a DragValue, so
    /// Settings shows no horizontal-drag cursor). Parsed on Apply.
    search_history_limit_buf: String,
    /// Buffer backing the auto-save interval input (minutes). Parsed + clamped
    /// to >= 1 on Apply. Only used when `auto_save_enabled`.
    auto_save_interval_buf: String,
    /// Buffer backing the chat temperature input (text, not a slider, so
    /// Settings shows no drag cursor). Parsed + clamped 0.0..=2.0 on Apply.
    /// Legacy: temperature is per profile now, so this only seeds a migration.
    chat_temperature_buf: String,

    // --- Chat profile add/edit form ---
    // The form doubles as "add" and "edit": a non-empty `chat_profile_form_id`
    // means we are editing that existing profile, empty means adding a new one.
    /// Id of the profile being edited; empty when adding a new one.
    chat_profile_form_id: String,
    chat_profile_form_name: String,
    chat_profile_form_desc: String,
    chat_profile_form_kind: ChatProviderKind,
    chat_profile_form_model: String,
    /// Comma-tolerant text buffer (Settings never shows a drag cursor).
    chat_profile_form_temp: String,
    chat_profile_form_reasoning: String,
    /// OpenAI's `text.verbosity` for this profile; empty means "do not send".
    chat_profile_form_verbosity: String,
    /// OpenAI Pro reasoning mode for this profile.
    chat_profile_form_pro_mode: bool,
    chat_profile_form_base_url: String,
    chat_profile_form_use_own_key: bool,
    chat_profile_form_allow_writes: bool,
    /// Password buffer for a profile's own key. Never populated from storage;
    /// typing into it is the only way it gains a value.
    chat_profile_form_key: String,
    /// Inline result of the last profile save (validation error or confirmation).
    chat_profile_status: Option<String>,
    /// Buffer backing the chat max-tool-iterations input. Parsed + clamped
    /// 1..=30 on Apply.
    chat_max_iterations_buf: String,
    /// Buffer backing the chat max-response-tokens input. Comma-tolerant
    /// integer; ignored when `chat_unlimited_tokens` is checked. Parsed on Apply.
    chat_max_tokens_buf: String,
    /// Buffer backing the chat result-row-limit input (`chat_result_row_limit`).
    /// Comma-tolerant integer (>= 1); ignored when `chat_unlimited_rows` is
    /// checked. Parsed on Apply.
    chat_result_row_limit_buf: String,
    /// Mirrors `AppSettings.chat_result_row_limit_unlimited`: when true the row
    /// limit input is greyed out and Apply writes the unlimited flag.
    chat_unlimited_rows: bool,
    /// Mirrors `AppSettings.chat_max_tokens_unlimited`: when true the cap input
    /// is greyed out and Apply writes the unlimited flag.
    chat_unlimited_tokens: bool,
    /// Audit-log size-warning threshold in MB (text buffer; parsed on Apply
    /// into `chat_audit_log_warn_bytes`).
    chat_audit_warn_mb_buf: String,
    /// Which provider's shared key the "API keys" sub-section edits. Its own
    /// picker: `AppSettings.chat_provider` is a dead migration field now that
    /// the provider lives on each profile, so it must not address the key form.
    chat_key_provider: ChatProviderKind,
    /// Masked API-key entry buffer for the chat provider, in the Chat section.
    chat_key_input_buf: String,
    /// Last "where the key was stored" status line after a Save/Clear.
    chat_key_status_msg: Option<String>,
    /// Set when the user clicks "Clear" on a chat API key: holds the provider
    /// awaiting deletion confirmation. `None` = no pending confirmation. Guards
    /// against an accidental one-click key wipe.
    chat_key_clear_confirm: Option<ChatProviderKind>,
    /// A connection test the profile form wants run, for the app to pick up.
    /// `None` between tests; the app `take()`s it.
    pub chat_test_request: Option<ChatTestRequest>,
    /// Slot the in-flight test writes into. `Some` while a test is running.
    chat_test_result: Option<ChatTestSlot>,
    /// Last test outcome: `(succeeded, message)`.
    chat_test_msg: Option<(bool, String)>,
    /// i18n key of the "what to fix" hint for the last failed test, if the
    /// error was recognisable. See [`chat_troubleshoot::hint_for`].
    chat_test_hint: Option<&'static str>,
    /// Set by the chat panel's Settings button so the Chat section opens
    /// expanded; consumed (reset) once the dialog has honoured it.
    pub focus_chat_section: bool,
    /// One-shot: also expand the Model profiles sub-section inside Chat
    /// (set alongside `focus_chat_section`; consumed on render).
    pub focus_chat_profiles: bool,
    /// Set by the sidebar's "Add connection" button so the Cloud storage
    /// section opens expanded; consumed (reset) once the dialog has honoured it.
    pub focus_cloud_section: bool,
    /// Set when the user unticks "Ask about redirects", cleared by the warning
    /// modal. Turning a safety check off is a decision worth explaining once.
    pub confirm_url_redirect_disable: bool,
    /// One-shot: also expand the add/edit-connection sub-section inside
    /// Cloud storage (the sidebar's "+ Add" sets it; consumed on render).
    pub focus_cloud_form: bool,
    /// When the user clicks "Record" for a shortcut, the action is stored here
    /// and the next key press captures a new binding. `None` = not recording.
    recording: Option<ShortcutAction>,
    /// Set when the user tries to bind a combo that is already used by another
    /// action. Cleared when they record successfully or edit the grid again.
    shortcut_conflict: Option<String>,
    /// The pending "that key is taken - take it over?" offer that goes with
    /// `shortcut_conflict`. `None` = nothing to decide.
    shortcut_takeover: Option<ShortcutTakeover>,
    /// Secrets deleted in the dialog, waiting to be deleted from the live
    /// settings too. Drained per frame by the app; see [`SecretPurge`].
    secret_purges: Vec<SecretPurge>,
    /// Whether the "Reset to defaults" confirmation modal is currently shown.
    show_reset_confirm: bool,
    /// Index of the connection currently loaded into the cloud form (edit
    /// mode); `None` = the form adds a new connection.
    cloud_editing: Option<usize>,
    /// Id of the connection being edited (empty = new); keeps the stable id
    /// across the form so its keyring secret stays addressable.
    cloud_form_id: String,
    cloud_form_name: String,
    cloud_form_kind: crate::cloud::CloudKind,
    cloud_form_bucket: String,
    cloud_form_region: String,
    cloud_form_endpoint: String,
    cloud_form_account: String,
    cloud_form_profile: String,
    cloud_form_path_style: bool,
    cloud_form_allow_http: bool,
    /// Public / anonymous access (skip signing; no secret or sign-in needed).
    cloud_form_anonymous: bool,
    /// Account-level connection: browse every bucket/container in the account.
    cloud_form_account_level: bool,
    /// Per-connection write permission (checked alongside the global cloud
    /// writes switch). Defaults false: a new connection is read-only until
    /// the user opts it in.
    cloud_form_allow_writes: bool,
    /// Optional key prefix to confine the connection to a folder in the bucket.
    cloud_form_prefix: String,
    /// GCS project id for account-level bucket listing (empty = active project).
    cloud_form_project: String,
    /// S3 access key id (secret entry).
    cloud_form_access_key_id: String,
    /// S3 secret access key / Azure account key / Azure SAS (per the toggle).
    cloud_form_secret: String,
    /// For Azure: the secret buffer is a SAS token rather than an account key.
    cloud_form_azure_is_sas: bool,
    /// BYO OAuth client-id buffer for native browser sign-in (Azure Blob / GCS
    /// fallback); maps to `CloudConnection.oauth_client_id`.
    cloud_form_oauth_client_id: String,
    /// Azure tenant buffer for native browser sign-in; maps to
    /// `CloudConnection.oauth_tenant`.
    cloud_form_oauth_tenant: String,
    /// Status line after a cloud secret save / clear.
    cloud_secret_status_msg: Option<String>,
    /// Armed Clear-secret confirmation: holds the connection id awaiting an
    /// explicit second click. Guards against a one-click secret wipe.
    cloud_secret_clear_confirm: Option<String>,
    /// In-flight cloud browser sign-in slot (Azure Blob / GCS).
    cloud_signin_result: Option<DbTestSlot>,
    /// Last finished cloud browser sign-in outcome (ok flag + message).
    cloud_signin_msg: Option<(bool, String)>,
    /// Set by the sidebar's Databases "+ Add" button so the Databases section
    /// opens expanded; consumed (reset) once the dialog has honoured it.
    pub focus_db_section: bool,
    /// Id of the DB connection being edited (empty = new); keeps the stable
    /// id across the form so its keyring secret stays addressable.
    db_form_id: String,
    db_form_name: String,
    db_form_engine: crate::db::DbEngine,
    db_form_host: String,
    /// Port as a text buffer; parsed on Save (falls back to the engine
    /// default on unparsable input).
    db_form_port: String,
    db_form_database: String,
    db_form_username: String,
    db_form_auth: crate::db::DbAuth,
    /// AWS region buffer for the IAM auth mode (empty = CLI default).
    db_form_region: String,
    /// IAM Identity Center (SSO) buffers for the AWS in-app browser sign-in;
    /// all empty = ambient AWS credentials (`aws sso login`).
    db_form_sso_start_url: String,
    db_form_sso_region: String,
    db_form_sso_account: String,
    db_form_sso_role: String,
    /// RSA private-key path buffer (Snowflake KeyPairJwt auth).
    db_form_private_key: String,
    /// OAuth client-id buffer (OAuthClientCredentials auth).
    db_form_client_id: String,
    /// OAuth token-URL buffer (OAuthClientCredentials auth; empty = default).
    db_form_token_url: String,
    /// Service-account key path buffer (BigQuery GcpServiceAccount auth).
    db_form_sa_key: String,
    /// BYO OAuth client-id buffer for native browser sign-in (Azure AD / GCP
    /// IAM fallback); maps to `DbConnection.oauth_client_id`.
    db_form_oauth_client_id: String,
    /// Query-timeout buffer in whole seconds; maps to
    /// `DbConnection.query_timeout_secs`. Shown only for the engines that
    /// poll for their results.
    db_form_query_timeout: String,
    /// Athena workgroup buffer; maps to `DbConnection.athena_workgroup`.
    db_form_athena_workgroup: String,
    /// Athena S3 result location buffer; maps to
    /// `DbConnection.athena_output_location`.
    db_form_athena_output: String,
    /// Azure tenant buffer for native browser sign-in; maps to
    /// `DbConnection.oauth_tenant`.
    db_form_oauth_tenant: String,
    db_form_allow_writes: bool,
    /// SSH-tunnel buffers: whether this connection goes through a jump host,
    /// and where it is. Mapped to `DbConnection.ssh` on Save.
    db_form_ssh_on: bool,
    db_form_ssh_host: String,
    db_form_ssh_port: String,
    db_form_ssh_user: String,
    db_form_ssh_auth: crate::db::ssh_tunnel::SshAuth,
    db_form_ssh_key_path: String,
    /// Bastion credential: the key's passphrase or the account password,
    /// depending on `db_form_ssh_auth`. Stored under its own keyring entry so
    /// it cannot overwrite the database secret.
    db_form_ssh_secret: String,
    db_form_ssh_accept_new: bool,
    /// Secret buffer (password / PAT / passphrase / client secret depending on
    /// the auth kind); stored to the keyring on Save.
    db_form_secret: String,
    /// Status line after a DB connection / secret save.
    db_secret_status_msg: Option<String>,
    /// Armed Clear-secret confirmation for the DB form.
    db_secret_clear_confirm: Option<String>,
    /// In-flight "Test connection" slot: Some while a test worker runs; the
    /// worker writes Ok(()) or Err(message) and the form drains it per frame.
    db_test_result: Option<DbTestSlot>,
    /// Last finished connection-test outcome (ok flag + message).
    db_test_msg: Option<(bool, String)>,
    /// In-flight browser sign-in slot (Azure AD / GCP IAM): the worker writes
    /// Ok(()) once the token is cached, or Err(message).
    db_signin_result: Option<DbTestSlot>,
    /// Last finished browser sign-in outcome (ok flag + message).
    db_signin_msg: Option<(bool, String)>,
    /// Window-size mode for the dialog (Normal / Maximized / Minimized).
    /// Persists across re-opens within the same app session - closing and
    /// reopening Settings keeps the size choice the user last picked.
    size: DialogSize,
}

impl SettingsDialog {
    /// Open the dialog, seeding the draft from current settings.
    pub fn open(&mut self, current: &AppSettings) {
        self.draft = current.clone();
        self.seed = current.clone();
        self.icon_changed = false;
        self.font_changed = false;
        self.theme_changed = false;
        self.seed_buffers();
        self.chat_key_input_buf.clear();
        self.chat_key_status_msg = None;
        self.chat_key_clear_confirm = None;
        self.chat_test_msg = None;
        self.chat_test_hint = None;
        self.clear_cloud_form();
        self.cloud_secret_status_msg = None;
        self.cloud_secret_clear_confirm = None;
        self.clear_db_form();
        // Reset here; the chat panel re-sets it to true right after calling
        // open() so the Chat section starts expanded only when launched there.
        self.focus_chat_section = false;
        // Same contract for the sidebar's "Add connection" button.
        self.focus_cloud_section = false;
        // And for the Databases tree's "+ Add" button.
        self.focus_db_section = false;
        self.recording = None;
        self.shortcut_conflict = None;
        self.shortcut_takeover = None;
        self.show_reset_confirm = false;
        self.open = true;
    }

    /// Take the secrets deleted since the last call, for the app to delete
    /// from the live settings too. See [`SecretPurge`].
    pub fn take_secret_purges(&mut self) -> Vec<SecretPurge> {
        std::mem::take(&mut self.secret_purges)
    }

    /// Record a deleted secret for the app to mirror into the live settings.
    pub(super) fn purge_secret(&mut self, purge: SecretPurge) {
        self.secret_purges.push(purge);
    }

    /// Whether the Shortcuts grid is waiting for a key press.
    ///
    /// The frame loop dispatches shortcuts before this dialog draws, so
    /// without this the key being recorded ALSO fired its current action:
    /// recording Ctrl+S saved the file, Ctrl+W closed the tab.
    pub fn is_recording_shortcut(&self) -> bool {
        self.recording.is_some()
    }

    /// Put the draft back to defaults, keeping the user's content.
    ///
    /// Saved connections, stored secrets, chat profiles and pinned tabs are
    /// work the user did, not values this dialog owns - and wiping the secrets
    /// here also orphaned their keyring entries, which no later Cancel could
    /// put back. Everything else, shortcuts included, goes back to default.
    pub(crate) fn reset_draft(&mut self) {
        let kept = std::mem::take(&mut self.draft);
        self.draft = AppSettings {
            cloud_connections: kept.cloud_connections,
            db_connections: kept.db_connections,
            cloud_secrets: kept.cloud_secrets,
            db_secrets: kept.db_secrets,
            chat_api_keys: kept.chat_api_keys,
            chat_profiles: kept.chat_profiles,
            chat_active_profile: kept.chat_active_profile,
            pinned_tabs: kept.pinned_tabs,
            last_release_notes_version: kept.last_release_notes_version,
            ..AppSettings::default()
        };
        self.seed_buffers();
    }

    /// Fill every text buffer / unit picker from the draft.
    ///
    /// Both opening the dialog and "Reset to defaults" need this, and reset
    /// used to re-seed only some of them: Apply parses **all** the buffers
    /// back over the draft, so ten settings (raw-view cap, decompression cap,
    /// grep size, chart caps, table-picker rows, Excel sheets, search history,
    /// auto-save interval, audit warning) quietly survived the reset.
    fn seed_buffers(&mut self) {
        let d = &self.draft;
        self.sql_row_limit_buf = d.sql_default_row_limit.to_string();
        self.write_row_group_buf = d
            .write_options
            .parquet
            .row_group_size
            .map(|n| n.to_string())
            .unwrap_or_default();
        // Pick the most natural unit for the current bytes value so the
        // user sees "1 MB" rather than "1,048,576 Bytes" when the setting
        // is at the default.
        self.syntax_highlight_size_unit = SizeUnit::best_fit(d.syntax_highlight_max_bytes);
        // `SizeUnit::factor` is always >= 1, so the division is safe.
        let unit_factor = self.syntax_highlight_size_unit.factor();
        let d = &self.draft;
        self.syntax_highlight_max_bytes_buf =
            crate::ui::status_bar::format_number(d.syntax_highlight_max_bytes / unit_factor);
        self.large_file_size_unit = SizeUnit::best_fit(d.large_file_min_bytes);
        self.large_file_min_bytes_buf = crate::ui::status_bar::format_number(
            d.large_file_min_bytes / self.large_file_size_unit.factor(),
        );
        self.initial_load_rows_buf = crate::ui::status_bar::format_number(d.initial_load_rows);
        self.db_page_rows_buf = crate::ui::status_bar::format_number(d.db_page_rows);
        self.raw_view_max_mb_buf =
            crate::ui::status_bar::format_number(d.raw_view_max_bytes / 1_000_000);
        self.max_decompressed_mb_buf =
            crate::ui::status_bar::format_number((d.max_decompressed_bytes / 1_000_000) as usize);
        self.folder_union_max_files_buf =
            crate::ui::status_bar::format_number(d.folder_union_max_files);
        self.text_mode_extensions_buf = d.text_mode_extensions.join(", ");
        // MCP buffers seed from the live settings.
        self.mcp_unlimited_rows = d.mcp_default_row_limit.is_none();
        self.mcp_row_limit_buf =
            crate::ui::status_bar::format_number(d.mcp_default_row_limit.unwrap_or(1000));
        self.mcp_cell_bytes_buf = crate::ui::status_bar::format_number(d.mcp_default_cell_bytes);
        self.grep_max_file_size_buf =
            crate::ui::status_bar::format_number(d.grep_max_file_size_mb as usize);
        self.chart_max_points_buf = crate::ui::status_bar::format_number(d.chart_max_points);
        self.chart_max_categories_buf =
            crate::ui::status_bar::format_number(d.chart_max_categories);
        self.table_picker_visible_rows_buf =
            crate::ui::status_bar::format_number(d.table_picker_visible_rows);
        self.excel_max_auto_sheets_buf =
            crate::ui::status_bar::format_number(d.excel_max_auto_sheets);
        self.search_history_limit_buf = d.search_history_limit.to_string();
        self.auto_save_interval_buf = d.auto_save_interval_minutes.to_string();
        self.status_message_secs_buf = d.status_message_secs.to_string();
        self.chat_temperature_buf = format!("{:.2}", d.chat_temperature);
        self.chat_max_iterations_buf = d.chat_max_tool_iterations.to_string();
        self.chat_max_tokens_buf = crate::ui::status_bar::format_number(d.chat_max_tokens);
        self.chat_result_row_limit_buf = d.chat_result_row_limit.to_string();
        self.chat_unlimited_rows = d.chat_result_row_limit_unlimited;
        self.chat_unlimited_tokens = d.chat_max_tokens_unlimited;
        self.chat_audit_warn_mb_buf = (d.chat_audit_log_warn_bytes / (1024 * 1024)).to_string();
    }

    /// Keep values that other surfaces also write across an Apply.
    ///
    /// The draft is seeded when the window opens and the app stays live behind
    /// it (the window can even be minimized), so committing the draft wholesale
    /// reverts everything written elsewhere in the meantime: a cloud secret
    /// cleared from the sidebar comes back, a tab pinned since the window
    /// opened is lost, the model picked in the chat panel snaps back.
    ///
    /// The rule is per field: if the dialog did not change it (draft still
    /// equals the seed), take whatever the app holds now. Only the fields
    /// another surface writes need listing - for every other field the live
    /// value and the seed are the same thing.
    pub fn carry_external_edits(&self, applied: &mut AppSettings, live: &AppSettings) {
        macro_rules! keep_live {
            ($($field:ident).+) => {
                if applied.$($field).+ == self.seed.$($field).+ {
                    applied.$($field).+ = live.$($field).+.clone();
                }
            };
        }
        // Written by `tabs.rs` (pin / unpin) and pruned by `update_loop.rs`.
        keep_live!(pinned_tabs);
        // Plaintext secret fallbacks: cleared by the sidebar's "Sign out" and
        // by the per-connection Delete buttons. Restoring one would put a
        // secret the user just cleared back into `settings.toml`.
        keep_live!(cloud_secrets);
        keep_live!(db_secrets);
        keep_live!(chat_api_keys);
        // The chat panel's own profile dropdown and Ollama model list.
        keep_live!(chat_active_profile);
        keep_live!(chat_profiles);
        // "Do not show this again" on the read-only notice.
        keep_live!(show_readonly_notice);
        // Remembered answer to the .xlsx formatting-export prompt.
        keep_live!(write_options.xlsx.include_formatting);
        // Ticked away on the release-notes window.
        keep_live!(last_release_notes_version);
    }

    /// Draw the dialog. Returns `Some(settings)` when the user clicks Apply.
    /// `logo` is an optional texture (the app icon) rendered as a header; passing
    /// `None` omits it and shows just the title.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        logo: Option<&egui::TextureHandle>,
    ) -> Option<AppSettings> {
        if !self.open {
            return None;
        }

        let mut applied: Option<AppSettings> = None;

        // Render the reset-confirm modal first so it sits above the Settings
        // window in the same frame.
        self.draw_reset_confirm(ctx);
        self.draw_url_redirect_disable_confirm(ctx);

        // Custom title bar (egui's is disabled below) - we render Min /
        // Max / Close buttons inline next to the title, like a typical
        // desktop window. Dragging works because the title text is a
        // non-interactive area inside the window's drag region.
        let screen_center = ctx.content_rect().center();
        let default_pos = screen_center - egui::vec2(340.0, 290.0);
        let dialog_id = egui::Id::new("octa_settings_dialog");
        let size = self.size;
        let window = egui::Window::new("Settings")
            .title_bar(false)
            .collapsible(false);
        let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
            w.resizable(true)
                .default_pos(default_pos)
                .min_width(640.0)
                .default_width(680.0)
                .default_height(580.0)
                .min_height(360.0)
        });
        let minimized = size == DialogSize::Minimized;
        let inner = window.show(ctx, |ui| {
            // Labels in Settings are static captions, not selectable text - turn
            // off egui's default label selection so hovering a row label (e.g.
            // "Temperature") shows the normal pointer instead of the text I-beam.
            // Input fields keep their own I-beam.
            ui.style_mut().interaction.selectable_labels = false;
            // Custom title bar: logo + "Octa Settings" + three control
            // buttons. Stays rendered when minimized so the user can
            // restore from there.
            egui::Panel::top("settings_header")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(tex) = logo {
                            let size = egui::vec2(28.0, 28.0);
                            ui.add(egui::Image::new(tex).fit_to_exact_size(size));
                            ui.add_space(8.0);
                        }
                        ui.label(
                            egui::RichText::new(crate::i18n::t("settings.window_title"))
                                .strong()
                                .size(16.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if draw_window_controls(ui, &mut self.size) {
                                self.open = false;
                            }
                        });
                    });
                });

            if minimized {
                return;
            }

            // Pin Apply/Cancel to the bottom so they're always reachable
            // regardless of how much content the scroll area holds.
            egui::Panel::bottom("settings_buttons")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::t("common.apply")).clicked() {
                            if let Ok(n) = parse_comma_number(&self.sql_row_limit_buf)
                                && n >= 1
                            {
                                self.draft.sql_default_row_limit = n;
                            }
                            if let Ok(n) = parse_comma_number(&self.syntax_highlight_max_bytes_buf)
                            {
                                // 0 is a valid input meaning "disable highlighting"
                                // - anything <= 0 trips the size guard immediately.
                                let unit_factor = self.syntax_highlight_size_unit.factor();
                                self.draft.syntax_highlight_max_bytes =
                                    n.saturating_mul(unit_factor);
                            }
                            if let Ok(n) = parse_comma_number(&self.large_file_min_bytes_buf) {
                                let unit_factor = self.large_file_size_unit.factor();
                                self.draft.large_file_min_bytes = n.saturating_mul(unit_factor);
                            }
                            if let Ok(n) = parse_comma_number(&self.initial_load_rows_buf)
                                && n >= 1
                            {
                                self.draft.initial_load_rows = n;
                            }
                            if let Ok(n) = parse_comma_number(&self.db_page_rows_buf)
                                && n >= 1
                            {
                                self.draft.db_page_rows = n;
                            }
                            // Raw-view size cap, entered in whole MB, stored
                            // in bytes. 0 is valid ("never load raw text").
                            if let Ok(mb) = parse_comma_number(&self.raw_view_max_mb_buf) {
                                self.draft.raw_view_max_bytes = mb.saturating_mul(1_000_000);
                            }
                            // Decompression cap, entered in whole MB, stored
                            // in bytes.
                            if let Ok(mb) = parse_comma_number(&self.max_decompressed_mb_buf) {
                                self.draft.max_decompressed_bytes =
                                    (mb as u64).saturating_mul(1_000_000);
                            }
                            // Folder-union file cap. The "Unlimited" checkbox
                            // writes itself; the number stays as the value to
                            // fall back on when it is unticked again.
                            if let Ok(n) = parse_comma_number(&self.folder_union_max_files_buf)
                                && n >= 1
                            {
                                self.draft.folder_union_max_files = n;
                            }
                            self.draft.text_mode_extensions = self
                                .text_mode_extensions_buf
                                .split([',', ' ', '\t', '\n'])
                                .map(|s| s.trim().trim_start_matches('.').to_lowercase())
                                .filter(|s| !s.is_empty())
                                .collect();
                            // MCP row cap: "Unlimited" overrides the text
                            // input, otherwise parse the comma-separated
                            // number. Invalid input falls back to the
                            // existing draft value so the user doesn't
                            // silently lose their previous setting.
                            if self.mcp_unlimited_rows {
                                self.draft.mcp_default_row_limit = None;
                            } else if let Ok(n) = parse_comma_number(&self.mcp_row_limit_buf)
                                && n >= 1
                            {
                                self.draft.mcp_default_row_limit = Some(n);
                            }
                            if let Ok(n) = parse_comma_number(&self.mcp_cell_bytes_buf) {
                                self.draft.mcp_default_cell_bytes = n;
                            }
                            // Multi-search per-file size cap. Stored as u32
                            // because mb >= 4 GB is nonsense for this knob.
                            // The "Unlimited" checkbox writes itself; the
                            // number stays as the value to fall back on when
                            // it is unticked again.
                            if let Ok(n) = parse_comma_number(&self.grep_max_file_size_buf)
                                && n >= 1
                            {
                                self.draft.grep_max_file_size_mb = n.min(u32::MAX as usize) as u32;
                            }
                            if let Ok(n) = parse_comma_number(&self.chart_max_points_buf) {
                                self.draft.chart_max_points = n;
                            }
                            if let Ok(n) = parse_comma_number(&self.chart_max_categories_buf) {
                                self.draft.chart_max_categories = n.max(1);
                            }
                            if let Ok(n) = parse_comma_number(&self.table_picker_visible_rows_buf) {
                                self.draft.table_picker_visible_rows = n.max(1);
                            }
                            if let Ok(n) = parse_comma_number(&self.excel_max_auto_sheets_buf) {
                                self.draft.excel_max_auto_sheets = n.max(1);
                            }
                            // Search history size: 0 is valid (disables history).
                            if let Ok(n) = parse_comma_number(&self.search_history_limit_buf) {
                                self.draft.search_history_limit = n;
                            }
                            // Auto-save interval: minutes, clamped to >= 1.
                            if let Ok(n) = parse_comma_number(&self.status_message_secs_buf) {
                                // Floored again at read time by
                                // `status_message_duration`; clamping here too
                                // means the value written to settings.toml is
                                // the one actually in force.
                                self.draft.status_message_secs =
                                    (n as u64).max(crate::ui::settings::MIN_STATUS_MESSAGE_SECS);
                            }
                            if let Ok(n) = parse_comma_number(&self.auto_save_interval_buf) {
                                self.draft.auto_save_interval_minutes = (n as u32).max(1);
                            }
                            // Chat temperature / iterations: parse + clamp;
                            // invalid input keeps the existing draft value.
                            if let Ok(t) = self.chat_temperature_buf.trim().parse::<f32>() {
                                self.draft.chat_temperature = t.clamp(0.0, 2.0);
                            }
                            if let Ok(n) = self.chat_max_iterations_buf.trim().parse::<usize>() {
                                self.draft.chat_max_tool_iterations = n.clamp(1, 30);
                            }
                            // Chat response-token cap: "Unlimited" overrides the
                            // text input; otherwise parse the comma-separated
                            // number (invalid input keeps the existing value).
                            self.draft.chat_max_tokens_unlimited = self.chat_unlimited_tokens;
                            if let Ok(n) = parse_comma_number(&self.chat_max_tokens_buf)
                                && n >= 1
                            {
                                self.draft.chat_max_tokens = n;
                            }
                            // Chat result-row limit: "Unlimited" checkbox
                            // overrides the number; otherwise parse (>= 1).
                            self.draft.chat_result_row_limit_unlimited = self.chat_unlimited_rows;
                            if let Ok(n) = parse_comma_number(&self.chat_result_row_limit_buf)
                                && n >= 1
                            {
                                self.draft.chat_result_row_limit = n;
                            }
                            // Audit-log warning threshold (MB -> bytes).
                            if let Ok(mb) = parse_comma_number(&self.chat_audit_warn_mb_buf) {
                                self.draft.chat_audit_log_warn_bytes = (mb as u64) * 1024 * 1024;
                            }
                            // Re-seed if the user deleted every profile, and
                            // re-point a dangling active id, so the assistant
                            // always comes back to a usable model.
                            crate::ui::settings::chat_profiles::ensure_profiles(&mut self.draft);
                            applied = Some(self.draft.clone());
                            self.open = false;
                        }
                        if ui.button(crate::i18n::t("common.cancel")).clicked() {
                            self.open = false;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label =
                                egui::RichText::new(crate::i18n::t("settings.reset_to_defaults"))
                                    .color(ui.visuals().error_fg_color);
                            if ui.button(label).clicked() {
                                self.show_reset_confirm = true;
                            }
                        });
                    });
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::default())
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            self.draw_sections(ui);
                        });
                });
        });

        if let Some(inner) = inner {
            remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
        }

        applied
    }

    /// Render the "Reset to defaults?" confirmation modal. On confirm, the
    /// draft is replaced with `AppSettings::default()` and the icon/font/theme
    /// changed flags are set so the existing Apply path re-applies them.
    /// Nothing is written to disk and the Settings window stays open - the
    /// user still has to click Apply (or Cancel) to commit / discard.
    /// Explain what turning off the redirect confirmation means, once.
    ///
    /// A forced choice rather than a toast: the switch removes the only place
    /// a changed destination becomes visible, so it should not be possible to
    /// flip it without reading why.
    fn draw_url_redirect_disable_confirm(&mut self, ctx: &egui::Context) {
        if !self.confirm_url_redirect_disable {
            return;
        }
        let mut turn_off = false;
        let mut keep = false;
        let dialog_id = egui::Id::new("octa_url_redirect_off_dialog");
        let size_key = dialog_id.with("octa_dlg_size");
        let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
        let minimized = size == DialogSize::Minimized;
        let mut chrome_close = false;

        let center = center_on_first_show(ctx, egui::vec2(480.0, 280.0));
        let window = egui::Window::new("octa_url_redirect_off")
            .id(dialog_id)
            .title_bar(false)
            .collapsible(false);
        let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
            w.resizable(true)
                .default_width(480.0)
                .default_height(280.0)
                .min_width(340.0)
                .min_height(170.0)
                .default_pos(center)
        });
        let inner = window.show(ctx, |ui| {
            egui::Panel::top("url_redirect_off_header")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(crate::i18n::t("settings.url_redirect_off_title"))
                                .strong()
                                .size(16.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if draw_window_controls(ui, &mut size) {
                                chrome_close = true;
                            }
                        });
                    });
                });
            if minimized {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| {
                ui.set_max_width(460.0);
                ui.label(crate::i18n::t("settings.url_redirect_off_body"));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(crate::i18n::t("settings.url_redirect_off_warn"))
                        .color(ui.visuals().warn_fg_color),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(crate::i18n::t("settings.url_redirect_off_keep"))
                        .clicked()
                    {
                        keep = true;
                    }
                    if ui
                        .button(crate::i18n::t("settings.url_redirect_off_confirm"))
                        .clicked()
                    {
                        turn_off = true;
                    }
                });
            });
        });
        if let Some(inner) = inner {
            remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
        }
        ctx.data_mut(|d| {
            d.insert_temp(
                size_key,
                if chrome_close {
                    DialogSize::Normal
                } else {
                    size
                },
            )
        });
        if chrome_close {
            keep = true;
        }
        if turn_off {
            self.confirm_url_redirect_disable = false;
        } else if keep {
            // Put the tick back: the switch only moves on an explicit choice.
            self.draft.confirm_url_redirects = true;
            self.confirm_url_redirect_disable = false;
        }
    }

    fn draw_reset_confirm(&mut self, ctx: &egui::Context) {
        if !self.show_reset_confirm {
            return;
        }
        let mut confirm = false;
        let mut cancel = false;
        let dialog_id = egui::Id::new("octa_settings_reset_dialog");
        let size_key = dialog_id.with("octa_dlg_size");
        let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
        let minimized = size == DialogSize::Minimized;
        let mut chrome_close = false;

        let center = center_on_first_show(ctx, egui::vec2(440.0, 220.0));
        let window = egui::Window::new("octa_settings_reset")
            .id(dialog_id)
            .title_bar(false)
            .collapsible(false);
        let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
            w.resizable(true)
                .default_width(440.0)
                .default_height(220.0)
                .min_width(320.0)
                .min_height(150.0)
                .default_pos(center)
        });
        let inner = window.show(ctx, |ui| {
            egui::Panel::top("settings_reset_header")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(crate::i18n::t("settings.reset_confirm_title"))
                                .strong()
                                .size(16.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if draw_window_controls(ui, &mut size) {
                                chrome_close = true;
                            }
                        });
                    });
                });
            if minimized {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label(crate::i18n::t("settings.reset_confirm_body"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::t("settings.reset")).clicked() {
                        confirm = true;
                    }
                    if ui.button(crate::i18n::t("common.cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        });
        if let Some(inner) = inner {
            remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
        }
        ctx.data_mut(|d| {
            d.insert_temp(
                size_key,
                if chrome_close {
                    DialogSize::Normal
                } else {
                    size
                },
            )
        });
        if chrome_close {
            cancel = true;
        }
        if confirm {
            self.reset_draft();
            self.icon_changed = true;
            self.font_changed = true;
            self.theme_changed = true;
            self.show_reset_confirm = false;
        } else if cancel {
            self.show_reset_confirm = false;
        }
    }

    /// Render the collapsible setting groups inside the scroll area.
    fn draw_sections(&mut self, ui: &mut egui::Ui) {
        // ── Appearance ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_appearance"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_appearance")
        .default_open(false)
        .show(ui, |ui| {
            self.appearance_section_body(ui);
        });

        // ── Files ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_files"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_files")
        .default_open(false)
        .show(ui, |ui| {
            self.files_section_body(ui);
        });

        // ── File-Specific ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_file_specific"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_format")
        .default_open(false)
        .show(ui, |ui| {
            self.format_section_body(ui);
        });

        // ── Table View ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_table_view"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_table")
        .default_open(false)
        .show(ui, |ui| {
            self.table_section_body(ui);
        });

        // ── Summary ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_summary"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_summary")
        .default_open(false)
        .show(ui, |ui| {
            self.summary_section_body(ui);
        });

        // ── Search & Editor ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_search_editor"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_search_editor")
        .default_open(false)
        .show(ui, |ui| {
            self.search_editor_section_body(ui);
        });

        // ── SQL ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_sql"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_sql")
        .default_open(false)
        .show(ui, |ui| {
            self.sql_section_body(ui);
        });

        // ── MCP server ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_mcp"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_mcp")
        .default_open(false)
        .show(ui, |ui| {
            self.mcp_section_body(ui);
        });

        // ── Chat / Assistant ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_chat"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_chat")
        // Opens expanded only when launched from the chat panel's Settings
        // button (consumes the one-shot flag; also opens the profiles
        // sub-section below, which is what that button is usually after).
        .default_open({
            let focus = std::mem::take(&mut self.focus_chat_section);
            self.focus_chat_profiles = self.focus_chat_profiles || focus;
            focus
        })
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::t("settings_hint.chat_intro"))
                    .weak()
                    .size(11.0),
            );
            ui.add_space(6.0);

            // The section splits into three sub-menus so per-profile options
            // (the profile list + form), global options, and API keys don't
            // pile into one overwhelming scroll.
            //
            // Model profiles: provider, model, temperature, thinking and the
            // write permission are all per profile, so they live in the
            // profile form rather than as single global fields here.
            egui::CollapsingHeader::new(
                egui::RichText::new(crate::i18n::t("chat.profiles")).strong(),
            )
            .id_salt("settings_chat_sub_profiles")
            .default_open(std::mem::take(&mut self.focus_chat_profiles))
            .show(ui, |ui| {
                self.chat_profiles_section(ui);
            });

            egui::CollapsingHeader::new(
                egui::RichText::new(crate::i18n::t("settings.sub_chat_global")).strong(),
            )
            .id_salt("settings_chat_sub_global")
            .show(ui, |ui| {
                self.chat_global_options_grid(ui);
            });

            egui::CollapsingHeader::new(
                egui::RichText::new(crate::i18n::t("settings.sub_api_keys")).strong(),
            )
            .id_salt("settings_chat_sub_keys")
            .show(ui, |ui| {
                self.chat_api_keys_body(ui);
            });
        });

        // ── Cloud storage ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_cloud"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_cloud")
        .default_open(std::mem::take(&mut self.focus_cloud_section))
        .show(ui, |ui| {
            self.cloud_section_body(ui);
        });

        // ── Databases ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_db"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_db")
        .default_open(std::mem::take(&mut self.focus_db_section))
        .show(ui, |ui| {
            self.db_section_body(ui);
        });

        // ── Map ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_map"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_map")
        .default_open(false)
        .show(ui, |ui| {
            self.map_section_body(ui);
        });

        // ── Directory Tree ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_directory_tree"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_directory_tree")
        .default_open(false)
        .show(ui, |ui| {
            self.directory_tree_section_body(ui);
        });

        // ── Shortcuts ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_shortcuts"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_shortcuts")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::t("settings_hint.shortcuts_intro"))
                    .weak()
                    .size(11.0),
            );
            ui.add_space(6.0);
            self.draw_shortcuts_grid(ui);
        });

        // ── Performance ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_performance"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_performance")
        .default_open(false)
        .show(ui, |ui| {
            self.performance_section_body(ui);
        });

        // ── Window ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_window"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_window")
        .default_open(false)
        .show(ui, |ui| {
            self.window_section_body(ui);
        });

        // ── Updates ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("release.section"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_updates")
        .default_open(false)
        .show(ui, |ui| {
            self.updates_section_body(ui);
        });

        // ── Diagnostics ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("diagnostics.section"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_diagnostics")
        .default_open(false)
        .show(ui, |ui| {
            self.diagnostics_section_body(ui);
        });
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;
    use crate::ui::settings::AppSettings;

    #[test]
    fn apply_keeps_settings_written_outside_the_dialog() {
        // The dialog opened, then the sidebar cleared a saved cloud secret and a
        // tab got pinned. Applying must not resurrect the secret or drop the pin.
        let mut dialog = SettingsDialog::default();
        dialog.open(&AppSettings::default());
        dialog
            .draft
            .cloud_secrets
            .insert("conn".into(), "sekrit".into());
        dialog
            .seed
            .cloud_secrets
            .insert("conn".into(), "sekrit".into());

        let mut live = dialog.seed.clone();
        live.cloud_secrets.remove("conn");
        live.pinned_tabs.push("/data/sales.parquet".into());

        let mut applied = dialog.draft.clone();
        dialog.carry_external_edits(&mut applied, &live);

        assert!(applied.cloud_secrets.is_empty(), "cleared secret came back");
        assert_eq!(applied.pinned_tabs, live.pinned_tabs, "pin was reverted");
    }

    #[test]
    fn apply_still_wins_for_fields_the_dialog_changed() {
        // Same contested field, but this time the user edited it in the dialog:
        // their choice must survive whatever the live settings hold.
        let mut dialog = SettingsDialog::default();
        dialog.open(&AppSettings::default());
        dialog.draft.show_readonly_notice = false;

        let mut live = dialog.seed.clone();
        live.show_readonly_notice = true;

        let mut applied = dialog.draft.clone();
        dialog.carry_external_edits(&mut applied, &live);

        assert!(
            !applied.show_readonly_notice,
            "the dialog's own edit was lost"
        );
    }

    #[test]
    fn reset_to_defaults_keeps_connections_and_secrets() {
        let mut settings = AppSettings {
            font_size: 22.0,
            grep_max_file_size_mb: 999,
            ..Default::default()
        };
        settings.cloud_secrets.insert("s3".into(), "sekrit".into());
        settings.pinned_tabs.push("/data/sales.parquet".into());

        let mut dialog = SettingsDialog::default();
        dialog.open(&settings);
        dialog.reset_draft();

        assert_eq!(dialog.draft.font_size, AppSettings::default().font_size);
        assert_eq!(
            dialog.draft.cloud_secrets.get("s3").map(String::as_str),
            Some("sekrit"),
            "a reset must not orphan the keyring entry it cannot restore"
        );
        assert_eq!(dialog.draft.pinned_tabs.len(), 1);
    }

    // Lives here rather than in `settings/mod_tests.rs` because it reads
    // `SettingsDialog`'s private buffer fields, which are visible only inside
    // this module and its children.
    #[test]
    fn reset_to_defaults_re_seeds_every_buffer() {
        // Apply parses all the text buffers back over the draft, so any buffer
        // the reset forgets silently restores the old value.
        let settings = AppSettings {
            grep_max_file_size_mb: 999,
            excel_max_auto_sheets: 42,
            auto_save_interval_minutes: 17,
            ..Default::default()
        };

        let mut dialog = SettingsDialog::default();
        dialog.open(&settings);
        assert_eq!(dialog.grep_max_file_size_buf, "999");
        dialog.reset_draft();

        let d = AppSettings::default();
        assert_eq!(
            dialog.grep_max_file_size_buf,
            d.grep_max_file_size_mb.to_string()
        );
        assert_eq!(
            dialog.excel_max_auto_sheets_buf,
            d.excel_max_auto_sheets.to_string()
        );
        assert_eq!(
            dialog.auto_save_interval_buf,
            d.auto_save_interval_minutes.to_string()
        );
    }
}
