pub mod chat_models;
mod dialog;
pub mod secrets;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::shortcuts::{ShortcutAction, Shortcuts};
use super::theme::{BodyFont, ThemeMode};
use crate::data::{BinaryDisplayMode, MapMode, MarkColor, SearchMode};

/// Layout for Jupyter notebook output cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotebookOutputLayout {
    /// Output shown beside the source cell (side by side).
    Beside,
    /// Output shown beneath the source cell (like Jupyter).
    #[default]
    Beneath,
}

impl NotebookOutputLayout {
    pub fn label(self) -> &'static str {
        match self {
            Self::Beside => "Beside",
            Self::Beneath => "Beneath",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Beside => "enum.nb_beside",
            Self::Beneath => "enum.nb_beneath",
        })
    }
}

/// Where to dock the directory tree sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DirectoryTreePosition {
    /// Docked to the left of the main area.
    #[default]
    Left,
    /// Docked to the right of the main area.
    Right,
}

impl DirectoryTreePosition {
    pub const ALL: &[DirectoryTreePosition] = &[Self::Left, Self::Right];

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Left => "enum.pos_left",
            Self::Right => "enum.pos_right",
        })
    }
}

/// Where to dock the SQL editor/result panel relative to the table view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SqlPanelPosition {
    /// Below the table (full width).
    #[default]
    Bottom,
    /// Above the table (full width).
    Top,
    /// To the left of the table (full height).
    Left,
    /// To the right of the table (full height).
    Right,
}

impl SqlPanelPosition {
    pub const ALL: &[SqlPanelPosition] = &[Self::Bottom, Self::Top, Self::Left, Self::Right];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bottom => "Bottom",
            Self::Top => "Top",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Bottom => "enum.pos_bottom",
            Self::Top => "enum.pos_top",
            Self::Left => "enum.pos_left",
            Self::Right => "enum.pos_right",
        })
    }
}

/// Which LLM provider the in-GUI chat panel talks to. One wire dialect per
/// variant; `OpenAiCompatible` reuses the OpenAI dialect against a
/// user-supplied `base_url` (Ollama / OpenRouter / Groq / LM Studio / ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChatProviderKind {
    /// Anthropic Messages API (Claude).
    #[default]
    Anthropic,
    /// OpenAI Chat Completions API.
    OpenAi,
    /// Any OpenAI-compatible endpoint reached via a configurable base URL.
    OpenAiCompatible,
    /// Google Gemini generateContent API.
    Gemini,
    /// Local Ollama server. Speaks the OpenAI dialect at `/v1`; Octa can
    /// start it in the background and list the models installed locally.
    Ollama,
}

impl ChatProviderKind {
    pub const ALL: &[ChatProviderKind] = &[
        Self::Ollama,
        Self::Anthropic,
        Self::OpenAi,
        Self::OpenAiCompatible,
        Self::Gemini,
    ];

    /// Stable identifier used as the key in `chat_models` / `chat_api_keys`
    /// and in the keyring entry name. Never change these (they are persisted).
    pub fn id(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::OpenAiCompatible => "openai_compat",
            Self::Gemini => "gemini",
            Self::Ollama => "ollama",
        }
    }

    /// Human-readable label for the provider picker.
    pub fn label(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic (Claude)",
            Self::OpenAi => "OpenAI",
            Self::OpenAiCompatible => "OpenAI-compatible",
            Self::Gemini => "Google Gemini",
            Self::Ollama => "Ollama (local)",
        }
    }

    /// The environment variable consulted first for this provider's key.
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi => "OPENAI_API_KEY",
            Self::OpenAiCompatible => "OCTA_OPENAI_COMPAT_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
            Self::Ollama => "OLLAMA_API_KEY",
        }
    }

    /// Whether this provider needs an API key. Ollama runs locally and does
    /// not, so the panel treats it as always ready.
    pub fn needs_api_key(self) -> bool {
        !matches!(self, Self::Ollama)
    }

    /// A sensible default model when the user has not picked one yet. Cheap
    /// models on purpose: a first chat should not surprise anyone on cost,
    /// and the dropdown makes the bigger models one click away.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-haiku-4-5-20251001",
            Self::OpenAi => "gpt-5.4-mini",
            Self::OpenAiCompatible => "llama3.1",
            Self::Gemini => "gemini-2.5-flash",
            Self::Ollama => "llama3.2",
        }
    }

    /// A short list of common / recent model names offered as quick picks in
    /// the model dropdown. Not exhaustive and model names change often, so the
    /// picker always keeps a free-text field for typing the exact current
    /// model. Ollama is dynamic (its list comes from `/api/tags`) and
    /// OpenAI-compatible depends on the endpoint, so both return an empty list.
    pub fn preset_models(self) -> &'static [&'static str] {
        match self {
            Self::Anthropic => &[
                "claude-fable-5",
                "claude-sonnet-4-6",
                "claude-opus-4-8",
                "claude-haiku-4-5-20251001",
            ],
            Self::OpenAi => &[
                "gpt-5.5",
                "gpt-5.5-pro",
                "gpt-5.4-mini",
                "gpt-5.4-nano",
                "gpt-5.2",
            ],
            Self::Gemini => &[
                "gemini-3.5-flash",
                "gemini-3.1-pro-preview",
                "gemini-3-flash-preview",
                "gemini-3.1-flash-lite",
                "gemini-2.5-pro",
                "gemini-2.5-flash",
            ],
            Self::OpenAiCompatible | Self::Ollama => &[],
        }
    }
}

/// Where to dock the chat panel relative to the table view. Mirrors
/// [`SqlPanelPosition`]; kept separate so the two panels can diverge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChatPanelPosition {
    /// To the right of the table (full height). The chat default.
    #[default]
    Right,
    /// To the left of the table (full height).
    Left,
    /// Below the table (full width).
    Bottom,
    /// Above the table (full width).
    Top,
}

impl ChatPanelPosition {
    pub const ALL: &[ChatPanelPosition] = &[Self::Right, Self::Left, Self::Bottom, Self::Top];

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Bottom => "enum.pos_bottom",
            Self::Top => "enum.pos_top",
            Self::Left => "enum.pos_left",
            Self::Right => "enum.pos_right",
        })
    }
}

/// Font used by the SQL editor's TextEdit and its gutter. Independent of the
/// table view's font setting so users who want a code-style monospace in the
/// editor but a different font everywhere else can have both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SqlEditorFont {
    /// Bundled JetBrains Mono Regular. Recommended for code legibility.
    #[default]
    JetBrainsMono,
    /// Reuse whatever family the rest of the UI uses (proportional or
    /// custom). Picks up the user's `FontSettings.body` and any custom path.
    MatchUiFont,
    /// egui's built-in monospace (Hack Regular). Lightest weight, no extra
    /// face registered.
    SystemMonospace,
}

impl SqlEditorFont {
    pub const ALL: &[SqlEditorFont] = &[
        Self::JetBrainsMono,
        Self::MatchUiFont,
        Self::SystemMonospace,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::JetBrainsMono => "JetBrains Mono (bundled)",
            Self::MatchUiFont => "Match UI font",
            Self::SystemMonospace => "System monospace",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::JetBrainsMono => "enum.sef_jetbrains",
            Self::MatchUiFont => "enum.sef_match_ui",
            Self::SystemMonospace => "enum.sef_system_mono",
        })
    }
}

/// Display unit for the syntax-highlight size cap in the Settings dialog.
/// Octa stores the cap as raw bytes in `settings.toml`; this enum only
/// governs how the value is presented and edited in the dialog. Not
/// persisted to the toml - defaults to MB at each open and the dialog
/// picks the most natural unit for the current value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyntaxSizeUnit {
    Bytes,
    KB,
    #[default]
    MB,
}

impl SyntaxSizeUnit {
    pub const ALL: &[SyntaxSizeUnit] = &[Self::Bytes, Self::KB, Self::MB];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bytes => "Bytes",
            Self::KB => "KB",
            Self::MB => "MB",
        }
    }

    pub fn label_t(self) -> String {
        match self {
            Self::Bytes => crate::i18n::t("enum.unit_bytes"),
            Self::KB => "KB".to_string(),
            Self::MB => "MB".to_string(),
        }
    }

    pub fn factor(self) -> usize {
        match self {
            Self::Bytes => 1,
            Self::KB => 1_024,
            Self::MB => 1_024 * 1_024,
        }
    }

    /// Pick the largest unit that represents `bytes` as an integer
    /// (so 1,048,576 -> 1 MB; 2,048 -> 2 KB; 1,500 -> 1500 Bytes).
    pub fn best_fit(bytes: usize) -> Self {
        if bytes == 0 {
            return Self::MB;
        }
        if bytes.is_multiple_of(Self::MB.factor()) {
            return Self::MB;
        }
        if bytes.is_multiple_of(Self::KB.factor()) {
            return Self::KB;
        }
        Self::Bytes
    }
}

/// Parse a string with optional comma thousand-separators into a `usize`.
/// Empty after stripping commas -> Err. Used by the Performance settings
/// inputs so users can type "1,000,000" the same way Octa renders numbers
/// elsewhere in the UI.
pub fn parse_comma_number(s: &str) -> Result<usize, std::num::ParseIntError> {
    s.replace(',', "").trim().parse::<usize>()
}

/// Initial window size before maximizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WindowSize {
    /// 800 × 600
    W800x600,
    /// 1280 × 720
    W1280x720,
    /// 1920 × 1080
    W1920x1080,
    /// 2560 × 1440
    W2560x1440,
    /// 3840 × 2160 (4K)
    #[default]
    W3840x2160,
    /// 5120 × 2880 (5K)
    W5120x2880,
    /// 7680 × 4320 (8K)
    W7680x4320,
}

impl WindowSize {
    pub const ALL: &[WindowSize] = &[
        Self::W800x600,
        Self::W1280x720,
        Self::W1920x1080,
        Self::W2560x1440,
        Self::W3840x2160,
        Self::W5120x2880,
        Self::W7680x4320,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::W800x600 => "800 × 600",
            Self::W1280x720 => "1280 × 720",
            Self::W1920x1080 => "1920 × 1080 (FHD)",
            Self::W2560x1440 => "2560 × 1440 (QHD)",
            Self::W3840x2160 => "3840 × 2160 (4K)",
            Self::W5120x2880 => "5120 × 2880 (5K)",
            Self::W7680x4320 => "7680 × 4320 (8K)",
        }
    }

    pub fn dimensions(self) -> [f32; 2] {
        match self {
            Self::W800x600 => [800.0, 600.0],
            Self::W1280x720 => [1280.0, 720.0],
            Self::W1920x1080 => [1920.0, 1080.0],
            Self::W2560x1440 => [2560.0, 1440.0],
            Self::W3840x2160 => [3840.0, 2160.0],
            Self::W5120x2880 => [5120.0, 2880.0],
            Self::W7680x4320 => [7680.0, 4320.0],
        }
    }
}

/// Available icon color variants (matching assets/octa-*.svg files).
///
/// `Random` is a meta-variant: it stays as `Random` in the persisted settings,
/// but at every Octa launch it picks one of the concrete variants via
/// [`IconVariant::resolve`] and uses that for the actual app/window icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IconVariant {
    Random,
    Rose,
    Amber,
    Blue,
    Cyan,
    Emerald,
    Indigo,
    Lime,
    Orange,
    Purple,
    Red,
    Slate,
    Teal,
    White,
    Black,
    Pink,
}

impl IconVariant {
    pub const ALL: &[IconVariant] = &[
        Self::Random,
        Self::Rose,
        Self::Amber,
        Self::Blue,
        Self::Cyan,
        Self::Emerald,
        Self::Indigo,
        Self::Lime,
        Self::Orange,
        Self::Purple,
        Self::Red,
        Self::Slate,
        Self::Teal,
        Self::White,
        Self::Black,
        Self::Pink,
    ];

    /// All concrete (non-Random) variants - what `Random` rolls between.
    pub const CONCRETE: &[IconVariant] = &[
        Self::Rose,
        Self::Amber,
        Self::Blue,
        Self::Cyan,
        Self::Emerald,
        Self::Indigo,
        Self::Lime,
        Self::Orange,
        Self::Purple,
        Self::Red,
        Self::Slate,
        Self::Teal,
        Self::White,
        Self::Black,
        Self::Pink,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Random => "Random",
            Self::Rose => "Rose",
            Self::Amber => "Amber",
            Self::Blue => "Blue",
            Self::Cyan => "Cyan",
            Self::Emerald => "Emerald",
            Self::Indigo => "Indigo",
            Self::Lime => "Lime",
            Self::Orange => "Orange",
            Self::Purple => "Purple",
            Self::Red => "Red",
            Self::Slate => "Slate",
            Self::Teal => "Teal",
            Self::White => "White",
            Self::Black => "Black",
            Self::Pink => "Pink",
        }
    }

    /// Returns the SVG source for this icon variant (compile-time embedded).
    /// For `Random`, returns a multi-color rosette used only as a preview.
    /// Callers that render the actual app icon must call [`Self::resolve`] first.
    pub fn svg_source(self) -> &'static str {
        match self {
            Self::Random => include_str!("../../../assets/octa-random.svg"),
            Self::Rose => include_str!("../../../assets/octa-rose.svg"),
            Self::Amber => include_str!("../../../assets/octa-amber.svg"),
            Self::Blue => include_str!("../../../assets/octa-blue.svg"),
            Self::Cyan => include_str!("../../../assets/octa-cyan.svg"),
            Self::Emerald => include_str!("../../../assets/octa-emerald.svg"),
            Self::Indigo => include_str!("../../../assets/octa-indigo.svg"),
            Self::Lime => include_str!("../../../assets/octa-lime.svg"),
            Self::Orange => include_str!("../../../assets/octa-orange.svg"),
            Self::Purple => include_str!("../../../assets/octa-purple.svg"),
            Self::Red => include_str!("../../../assets/octa-red.svg"),
            Self::Slate => include_str!("../../../assets/octa-slate.svg"),
            Self::Teal => include_str!("../../../assets/octa-teal.svg"),
            Self::White => include_str!("../../../assets/octa-white.svg"),
            Self::Black => include_str!("../../../assets/octa-black.svg"),
            Self::Pink => include_str!("../../../assets/octa-pink.svg"),
        }
    }

    /// Resolve a concrete variant: returns `self` for any concrete variant; for
    /// `Random`, picks one of [`Self::CONCRETE`] uniformly at random.
    pub fn resolve(self) -> IconVariant {
        // rand 0.9+ moved `choose` to the `IndexedRandom` trait and renamed
        // the global RNG constructor from `thread_rng` to `rng`.
        use rand::seq::IndexedRandom;
        if self == Self::Random {
            *Self::CONCRETE
                .choose(&mut rand::rng())
                .unwrap_or(&Self::Rose)
        } else {
            self
        }
    }

    /// Preview color for the icon picker UI.
    pub fn preview_color(self) -> egui::Color32 {
        use egui::Color32;
        match self {
            Self::Random => Color32::from_rgb(0x99, 0x99, 0x99),
            Self::Rose => Color32::from_rgb(0xe1, 0x1d, 0x48),
            Self::Amber => Color32::from_rgb(0xf5, 0x9e, 0x0b),
            Self::Blue => Color32::from_rgb(0x3b, 0x82, 0xf6),
            Self::Cyan => Color32::from_rgb(0x06, 0xb6, 0xd4),
            Self::Emerald => Color32::from_rgb(0x10, 0xb9, 0x81),
            Self::Indigo => Color32::from_rgb(0x63, 0x66, 0xf1),
            Self::Lime => Color32::from_rgb(0x84, 0xcc, 0x16),
            Self::Orange => Color32::from_rgb(0xf9, 0x73, 0x16),
            Self::Purple => Color32::from_rgb(0xa8, 0x55, 0xf7),
            Self::Red => Color32::from_rgb(0xef, 0x44, 0x44),
            Self::Slate => Color32::from_rgb(0x64, 0x74, 0x8b),
            Self::Teal => Color32::from_rgb(0x14, 0xb8, 0xa6),
            Self::White => Color32::from_rgb(0xf8, 0xfa, 0xfc),
            Self::Black => Color32::from_rgb(0x0f, 0x17, 0x2a),
            Self::Pink => Color32::from_rgb(0xec, 0x48, 0x99),
        }
    }
}

/// Allocate a small filled square next to a label so the icon-color picker
/// can show its swatch without baking the color into the label text (which
/// would render `White` invisibly on light themes and `Black` on dark).
fn paint_icon_swatch(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
    ui.painter().rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Outside,
    );
}

/// Persistent application settings.
///
/// `#[serde(default)]` on the struct fills every missing field from
/// [`AppSettings::default`] when loading a TOML written by an older or newer
/// release. Combined with the parse-failure backup in [`AppSettings::load`],
/// this means upgrading Octa never silently wipes the user's settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Base font size in points (applied to Body, Button, Monospace).
    pub font_size: f32,
    /// Default theme when the application starts.
    pub default_theme: ThemeMode,
    /// Icon color variant.
    pub icon_variant: IconVariant,
    /// Default search mode for the filter bar.
    #[serde(default)]
    pub default_search_mode: SearchMode,
    /// Whether to show row numbers in the table view.
    #[serde(default = "default_true")]
    pub show_row_numbers: bool,
    /// When a filter is active, show a second row-number column counting the
    /// visible rows from 1 (alongside the original row numbers). Only appears
    /// while filtered; redundant otherwise.
    #[serde(default = "default_true")]
    pub show_sequential_row_numbers: bool,
    /// Whether to use alternating row background colors.
    #[serde(default = "default_true")]
    pub alternating_row_colors: bool,
    /// Whether negative numbers are displayed in red.
    #[serde(default)]
    pub negative_numbers_red: bool,
    /// Whether `Int` / `Float` cells render with thousand separators
    /// (e.g. `1,234,567.89`) in the table view. Display-only - never alters
    /// saved / exported data. Default `true`.
    #[serde(default = "default_true")]
    pub thousands_separators_in_cells: bool,
    /// Grouping / decimal-mark convention for numeric cells: English
    /// (`1,234.56`) or European (`1.234,56`). The decimal mark follows this
    /// even when `thousands_separators_in_cells` is off. Default English.
    #[serde(default)]
    pub number_separator_style: crate::data::num_format::SeparatorStyle,
    /// Default search behaviour: `Filter` hides non-matching rows (table only),
    /// `Highlight` keeps every row and highlights matches in place. The
    /// search-bar toggle overrides this per session. Text/tree views always
    /// highlight regardless.
    #[serde(default)]
    pub search_result_mode: crate::data::SearchResultMode,
    /// How many recent search queries to remember across sessions (the search
    /// box history dropdown). 0 disables the history. Default 20.
    #[serde(default = "default_search_history_limit")]
    pub search_history_limit: usize,
    /// Whether edited cells are highlighted with a background color.
    #[serde(default)]
    pub highlight_edits: bool,
    /// Whether to color columns differently in aligned raw CSV/TSV view.
    #[serde(default = "default_true")]
    pub color_aligned_columns: bool,
    /// Layout for Jupyter notebook output cells.
    #[serde(default)]
    pub notebook_output_layout: NotebookOutputLayout,
    /// Maximum number of recently opened files shown in the File menu.
    #[serde(default = "default_max_recent")]
    pub max_recent_files: usize,
    /// Whether to allow line breaks in table cells (wraps long text).
    #[serde(default)]
    pub cell_line_breaks: bool,
    /// How to display binary data columns (Binary, Hex, or Text).
    #[serde(default)]
    pub binary_display_mode: BinaryDisplayMode,
    /// Number of spaces inserted when pressing Tab in the text editor.
    #[serde(default = "default_tab_size")]
    pub tab_size: usize,
    /// Body / heading font choice (egui built-in proportional vs monospace).
    #[serde(default)]
    pub body_font: BodyFont,
    /// Optional path to a user-provided .ttf/.otf font. Overrides `body_font`
    /// for proportional text when set and readable.
    #[serde(default)]
    pub custom_font_path: String,
    /// Default color used by the `Mark` shortcut when the user has not picked
    /// a specific color via the toolbar / context menu.
    #[serde(default = "default_mark_color")]
    pub default_mark_color: MarkColor,
    /// Whether the SQL panel should be open by default when a tabular file is
    /// loaded.
    #[serde(default)]
    pub sql_panel_default_open: bool,
    /// Where to dock the SQL panel (Bottom or Right of the table view).
    #[serde(default)]
    pub sql_panel_position: SqlPanelPosition,
    /// Default LIMIT used in the placeholder query for new tabs.
    #[serde(default = "default_sql_row_limit")]
    pub sql_default_row_limit: usize,
    /// Whether the SQL editor offers keyword + column-name autocomplete.
    #[serde(default = "default_true")]
    pub sql_autocomplete: bool,
    /// Which font face the SQL editor (and its line-number gutter) uses.
    /// Independent of the UI font so users can keep the rest of Octa on a
    /// proportional face while reading SQL in monospace.
    #[serde(default)]
    pub sql_editor_font: SqlEditorFont,
    /// Briefly highlight the cells a SQL mutation (INSERT/UPDATE/DELETE)
    /// changed, so the effect of a query is visible. Default on.
    #[serde(default = "default_true")]
    pub sql_row_diff_highlight_enabled: bool,
    /// How long (seconds) the post-mutation row-diff highlight stays before
    /// fading. Default 4.
    #[serde(default = "default_sql_row_diff_secs")]
    pub sql_row_diff_highlight_secs: u32,
    /// Where to dock the directory tree sidebar when a folder is open.
    #[serde(default)]
    pub directory_tree_position: DirectoryTreePosition,
    /// Whether to show a confirmation warning before toggling "Align Columns"
    /// off in the raw CSV/TSV view, which reloads the file and discards edits.
    #[serde(default = "default_true")]
    pub warn_raw_align_reload: bool,
    /// Whether to show a one-shot banner when date inference promotes a string
    /// column to typed `Date`/`DateTime` AND the canonical ISO display format
    /// differs from the source format on disk (e.g. stored as `02.05.2026` but
    /// displayed as `2026-05-02`). The banner explains the change and offers
    /// a Dismiss button. Disable here to silence it globally.
    #[serde(default = "default_true")]
    pub warn_on_date_format_change: bool,
    /// User-customizable keyboard shortcut bindings.
    #[serde(default)]
    pub shortcuts: Shortcuts,
    /// Initial window size. Only has a visible effect when
    /// [`AppSettings::start_maximized`] is off; otherwise it is the
    /// restore-from-maximize size.
    #[serde(default)]
    pub window_size: WindowSize,
    /// Whether to launch the window maximized. When off, the window
    /// comes up at [`AppSettings::window_size`] instead.
    #[serde(default = "default_true")]
    pub start_maximized: bool,
    /// Whether to pop a confirmation modal each time read-only mode is
    /// toggled (via shortcut or menu). Setting to `false` silences the
    /// notice; the read-only state still flips, you just don't see the
    /// pop-up. Default `true`.
    #[serde(default = "default_true")]
    pub show_readonly_notice: bool,
    /// When `true`, Octa requests an undecorated viewport at startup and
    /// renders its own slim title bar (logo + title + min/max/close buttons),
    /// with drag-to-move and edge/corner resize handles. Default `true` - the
    /// in-toolbar controls free the vertical space a system title bar would
    /// take and look consistent across platforms; opt out for native window
    /// decorations.
    #[serde(default = "default_true")]
    pub use_custom_title_bar: bool,
    /// Hard cap (in bytes) for files where the raw editor still applies
    /// syntect syntax highlighting. Past this threshold the editor falls
    /// back to plain monospace because per-frame tokenisation gets laggy.
    /// Default 1 MB. Set to 0 to disable highlighting entirely; set very
    /// high to opt out of the guard.
    #[serde(default = "default_syntax_highlight_max_bytes")]
    pub syntax_highlight_max_bytes: usize,
    /// Maximum number of rows loaded into the active `DataTable` on first
    /// open for streaming formats (Parquet, CSV, TSV). Additional rows
    /// load in the background as the user scrolls toward the bottom.
    /// Default 5,000,000. Setting this very high improves first-paint
    /// completeness but uses more memory; setting it lower makes the
    /// initial open faster but means the background loader has to do
    /// more work as you scroll. Ignored when
    /// [`initial_load_rows_unlimited`](Self::initial_load_rows_unlimited)
    /// is `true`.
    #[serde(default = "default_initial_load_rows")]
    pub initial_load_rows: usize,
    /// When `true`, disables the initial-load cap entirely - every row in
    /// the file is loaded up front. Trumps [`initial_load_rows`](Self::initial_load_rows).
    /// Default `false`. Power users on machines with plenty of RAM can flip
    /// this on so a single huge parquet/CSV opens in one shot.
    #[serde(default)]
    pub initial_load_rows_unlimited: bool,
    /// User-extensible list of file extensions (no leading dot, lowercase)
    /// that Octa should treat as plain text. Files with these extensions
    /// are routed through `TextReader` regardless of any other reader that
    /// would normally claim them. Useful for unusual config or log
    /// extensions Octa doesn't ship native support for.
    #[serde(default)]
    pub text_mode_extensions: Vec<String>,
    /// Absolute paths of pinned tabs. Restored on next launch through the
    /// regular `load_file` path. Files that no longer exist on disk are
    /// silently dropped from this list. Unsaved changes in a pinned tab are
    /// **not** auto-saved at close - the standard unsaved-changes dialog
    /// still runs.
    #[serde(default)]
    pub pinned_tabs: Vec<String>,
    /// Default row cap applied by the MCP server (`octa --mcp`) when a tool
    /// call omits its `limit` parameter. `None` means "return every row";
    /// `Some(n)` caps the response and sets `truncated: true` in the JSON.
    /// Defaults to `Some(1000)`. Read once at server startup - changing this
    /// while a server is running needs an `octa --mcp` restart.
    #[serde(default = "default_mcp_row_limit")]
    pub mcp_default_row_limit: Option<usize>,
    /// Per-cell byte cap applied by the MCP server. Cells whose textual
    /// form exceeds this are replaced with a `[truncated: ...]` marker and
    /// the tool response flags `cell_truncated: true`. `0` means no cap.
    /// Default 65,536 bytes (64 KiB).
    #[serde(default = "default_mcp_cell_bytes")]
    pub mcp_default_cell_bytes: usize,
    /// Default rendering mode for the Map view when opening a GeoJSON file.
    /// `Tiles` shows a slippy-map background; `GeometryOnly` skips the
    /// network fetch and paints just the geometry on a blank canvas.
    /// Toggleable per tab via the Map toolbar.
    #[serde(default)]
    pub map_default_mode: MapMode,
    /// When the map mode is `Tiles` and tile fetching fails (offline / DNS
    /// block / server error), automatically fall back to geometry-only
    /// rendering instead of leaving the user staring at a grey grid.
    /// Default `true`.
    #[serde(default = "default_true")]
    pub map_fallback_to_geometry: bool,
    /// Tile URL template, `{z}/{x}/{y}` for zoom + tile coordinates. The
    /// default points at the OSM tile server - please honour the
    /// [OSM Tile Usage Policy](https://operations.osmfoundation.org/policies/tiles/)
    /// in production deployments (point at a self-hosted or commercial
    /// provider, or get an API key).
    #[serde(default = "default_map_tile_url")]
    pub map_tile_url_template: String,
    /// Per-file size cap (megabytes) for the directory scope of the
    /// multi-search panel. Files over this size are skipped silently
    /// during the scan. Default 50 MB. `0` disables the cap.
    #[serde(default = "default_grep_max_file_size_mb")]
    pub grep_max_file_size_mb: u32,
    /// Maximum number of input rows the Chart tab will plot before
    /// evenly-spaced downsampling kicks in. Histogram, Line, and Scatter
    /// all honour this; Bar always aggregates the full input and is
    /// bounded by `chart_max_categories` instead. Default 100,000.
    /// `0` disables sampling - at your own risk for very large tables.
    #[serde(default = "default_chart_max_points")]
    pub chart_max_points: usize,
    /// Maximum distinct X categories a Bar chart will accept. Above this
    /// the chart refuses to draw rather than rendering a wall of
    /// unreadable bars - the user should filter or group before charting.
    /// Default 200.
    #[serde(default = "default_chart_max_categories")]
    pub chart_max_categories: usize,
    /// How many sheets of a multi-sheet Excel workbook to open automatically
    /// (each in its own tab) without prompting. If a workbook has more sheets
    /// than this, Octa shows a sheet picker so the user chooses which to open
    /// (they may pick more than this number, or all of them). Default 5.
    #[serde(default = "default_excel_max_auto_sheets")]
    pub excel_max_auto_sheets: usize,
    /// Whether to strip leading/trailing whitespace from string cells when a
    /// file is loaded. Interior whitespace is untouched. Default `false` -
    /// loads leave cell values exactly as stored unless the user opts in.
    #[serde(default)]
    pub trim_whitespace_on_load: bool,
    /// Whether to normalise column headers to lower snake_case identifiers when
    /// a file is loaded (trim, lowercase, non-alphanumeric runs -> `_`,
    /// de-collide repeats with `_2`). Default `false` - headers load verbatim
    /// unless the user opts in. See `octa::data::trim::clean_headers`.
    #[serde(default)]
    pub clean_headers_on_load: bool,
    /// Whether to show a dismissible banner listing the columns that had
    /// whitespace trimmed on load. Default `true`. Independent of
    /// [`trim_whitespace_on_load`] - trimming can run silently if this is off.
    #[serde(default = "default_true")]
    pub warn_on_whitespace_trim: bool,
    /// How many table rows the multi-table picker dialog (SQLite / DuckDB /
    /// other multi-table sources) should fit vertically by default. The
    /// dialog stays user-resizable - this only controls the initial height
    /// so the picker doesn't dominate the screen when a database has a
    /// handful of tables. Default 10.
    #[serde(default = "default_table_picker_visible_rows")]
    pub table_picker_visible_rows: usize,
    /// UI language code (e.g. "en", "de", "fr"). Drives `octa::i18n`. Unknown
    /// or unsupported codes fall back to English at apply time. Default "en".
    #[serde(default = "default_language")]
    pub language: String,
    /// When a delimited text file (CSV / TSV) reads but looks malformed
    /// (invalid encoding, a leading BOM, control characters, a delimiter that
    /// disagrees with the extension, or wildly ragged rows), offer an
    /// interactive repair prompt instead of failing or loading garbage. Off by
    /// default so normal loads are never interrupted; opt in under
    /// Settings -> Performance. The repair itself (lossy decode, delimiter
    /// re-detection, BOM/control stripping) is only applied if the user
    /// confirms it in the prompt.
    #[serde(default)]
    pub offer_repair_on_malformed: bool,
    /// The currently selected chat provider. Keys and per-provider model
    /// choices are stored separately so switching providers never re-prompts.
    #[serde(default)]
    pub chat_provider: ChatProviderKind,
    /// Last-used model name per provider, keyed by [`ChatProviderKind::id`].
    /// Lets the panel restore each provider's model in one click.
    #[serde(default)]
    pub chat_models: std::collections::BTreeMap<String, String>,
    /// Base URL for the OpenAI-compatible provider (OpenRouter / Groq /
    /// LM Studio / a custom gateway). Ignored by the other providers.
    /// Example: `https://openrouter.ai/api/v1`.
    #[serde(default)]
    pub chat_base_url: String,
    /// Root URL of the local Ollama server (no `/v1` suffix). Octa appends
    /// `/v1/chat/completions` for chat and `/api/tags` for the model list.
    #[serde(default = "default_chat_ollama_url")]
    pub chat_ollama_url: String,
    /// Where to dock the chat panel. Default Right.
    #[serde(default)]
    pub chat_panel_position: ChatPanelPosition,
    /// Sampling temperature passed to the provider. Default 0.7.
    #[serde(default = "default_chat_temperature")]
    pub chat_temperature: f32,
    /// Maximum agentic tool-use iterations per turn before the loop stops.
    /// Guards against runaway tool loops. Default 3.
    #[serde(default = "default_chat_max_tool_iterations")]
    pub chat_max_tool_iterations: usize,
    /// Maximum response tokens requested from the provider per turn.
    /// Default 16384. Ignored when `chat_max_tokens_unlimited` is set.
    #[serde(default = "default_chat_max_tokens")]
    pub chat_max_tokens: usize,
    /// When `true`, no response-token cap is sent (providers use their own
    /// default; Anthropic, which requires the field, substitutes a high value).
    #[serde(default)]
    pub chat_max_tokens_unlimited: bool,
    /// Directory the chat assistant writes exported files into (new CSVs,
    /// charts, ...) when the model gives a bare filename. The agent can only
    /// write here or to an absolute path the user explicitly requests.
    #[serde(default = "default_chat_export_dir")]
    pub chat_export_dir: String,
    /// Record every assistant tool call to `<config_dir>/chat_audit/`. Off by
    /// default. See `src/app/chat/audit.rs`.
    #[serde(default)]
    pub chat_audit_log_enabled: bool,
    /// Warn at startup when the total size of the chat audit logs exceeds this
    /// many bytes. Default 10 MB.
    #[serde(default = "default_chat_audit_warn_bytes")]
    pub chat_audit_log_warn_bytes: u64,
    /// Whether the audit-log size warning is shown at all. Default on.
    #[serde(default = "default_true")]
    pub chat_audit_log_warn_enabled: bool,
    /// Plaintext per-provider API keys, keyed by [`ChatProviderKind::id`].
    /// Only populated when the OS keyring is unavailable; the keyring is
    /// preferred. Storing a key here means it sits in `settings.toml` in the
    /// clear, which the UI warns about explicitly.
    #[serde(default)]
    pub chat_api_keys: std::collections::BTreeMap<String, String>,
    /// Which statistics the Analyse -> Summary tab shows, in addition to the
    /// always-present column name and type. Defaults to the full set. See
    /// `src/data/summary.rs`.
    #[serde(default = "default_summary_stats")]
    pub summary_stats: Vec<crate::data::summary::SummaryStat>,
    /// When a folder is open in the directory-tree sidebar, show only
    /// sub-folders and files Octa can open (by extension). Default `true`.
    /// Turn off to list every file regardless of type.
    #[serde(default = "default_true")]
    pub directory_tree_filter_enabled: bool,
}

fn default_summary_stats() -> Vec<crate::data::summary::SummaryStat> {
    crate::data::summary::SummaryStat::default_enabled()
}

fn default_true() -> bool {
    true
}

fn default_chat_temperature() -> f32 {
    0.0
}

fn default_chat_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_chat_max_tool_iterations() -> usize {
    3
}

fn default_chat_max_tokens() -> usize {
    16_384
}

/// Default chat export directory: the user's Downloads folder if it exists,
/// otherwise the home directory. Empty string if neither resolves.
fn default_chat_audit_warn_bytes() -> u64 {
    10 * 1024 * 1024
}

fn default_chat_export_dir() -> String {
    if let Some(home) = dirs_path_home() {
        let downloads = home.join("Downloads");
        if downloads.is_dir() {
            return downloads.to_string_lossy().into_owned();
        }
        return home.to_string_lossy().into_owned();
    }
    String::new()
}

fn default_language() -> String {
    "en".to_string()
}

fn default_max_recent() -> usize {
    5
}

fn default_search_history_limit() -> usize {
    5
}

fn default_sql_row_diff_secs() -> u32 {
    4
}

fn default_tab_size() -> usize {
    4
}

fn default_sql_row_limit() -> usize {
    100
}

fn default_syntax_highlight_max_bytes() -> usize {
    1024 * 1024
}

fn default_initial_load_rows() -> usize {
    5_000_000
}

// Kept literal here (rather than referencing `crate::mcp::DEFAULT_*`)
// because `mcp` lives in the binary side of the crate split and the
// settings module is in the library. The values are mirrored by
// `src/mcp/mod.rs::DEFAULT_ROW_LIMIT` / `DEFAULT_CELL_BYTE_LIMIT`.
fn default_mcp_row_limit() -> Option<usize> {
    Some(1000)
}

fn default_mcp_cell_bytes() -> usize {
    64 * 1024
}

fn default_grep_max_file_size_mb() -> u32 {
    50
}

fn default_chart_max_points() -> usize {
    100_000
}

fn default_chart_max_categories() -> usize {
    crate::data::chart::DEFAULT_MAX_BAR_CATEGORIES
}

fn default_table_picker_visible_rows() -> usize {
    10
}

fn default_excel_max_auto_sheets() -> usize {
    5
}

fn default_map_tile_url() -> String {
    // Stock OSM tile server. Walkers ships a `sources::OpenStreetMap`
    // helper that points at the same URL; we duplicate the literal here
    // so the user can edit it without juggling a `walkers::sources` type.
    "https://tile.openstreetmap.org/{z}/{x}/{y}.png".to_string()
}

fn default_mark_color() -> MarkColor {
    MarkColor::Green
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            default_theme: ThemeMode::Light,
            icon_variant: IconVariant::Rose,
            default_search_mode: SearchMode::Plain,
            show_row_numbers: true,
            show_sequential_row_numbers: true,
            alternating_row_colors: true,
            negative_numbers_red: true,
            thousands_separators_in_cells: true,
            number_separator_style: crate::data::num_format::SeparatorStyle::default(),
            search_result_mode: crate::data::SearchResultMode::default(),
            search_history_limit: default_search_history_limit(),
            highlight_edits: false,
            cell_line_breaks: false,
            binary_display_mode: BinaryDisplayMode::default(),
            color_aligned_columns: true,
            notebook_output_layout: NotebookOutputLayout::default(),
            max_recent_files: 10,
            tab_size: 4,
            body_font: BodyFont::Proportional,
            custom_font_path: String::new(),
            default_mark_color: default_mark_color(),
            sql_panel_default_open: false,
            sql_panel_position: SqlPanelPosition::default(),
            sql_default_row_limit: 100,
            sql_autocomplete: true,
            sql_editor_font: SqlEditorFont::default(),
            sql_row_diff_highlight_enabled: true,
            sql_row_diff_highlight_secs: default_sql_row_diff_secs(),
            directory_tree_position: DirectoryTreePosition::default(),
            warn_raw_align_reload: true,
            warn_on_date_format_change: true,
            shortcuts: Shortcuts::default(),
            window_size: WindowSize::default(),
            start_maximized: true,
            show_readonly_notice: true,
            use_custom_title_bar: true,
            syntax_highlight_max_bytes: default_syntax_highlight_max_bytes(),
            initial_load_rows: default_initial_load_rows(),
            initial_load_rows_unlimited: false,
            text_mode_extensions: Vec::new(),
            pinned_tabs: Vec::new(),
            mcp_default_row_limit: default_mcp_row_limit(),
            mcp_default_cell_bytes: default_mcp_cell_bytes(),
            map_default_mode: MapMode::default(),
            map_fallback_to_geometry: true,
            map_tile_url_template: default_map_tile_url(),
            grep_max_file_size_mb: default_grep_max_file_size_mb(),
            chart_max_points: default_chart_max_points(),
            chart_max_categories: default_chart_max_categories(),
            table_picker_visible_rows: default_table_picker_visible_rows(),
            excel_max_auto_sheets: default_excel_max_auto_sheets(),
            trim_whitespace_on_load: false,
            clean_headers_on_load: false,
            warn_on_whitespace_trim: true,
            offer_repair_on_malformed: false,
            language: default_language(),
            chat_provider: ChatProviderKind::default(),
            chat_models: std::collections::BTreeMap::new(),
            chat_base_url: String::new(),
            chat_ollama_url: default_chat_ollama_url(),
            chat_panel_position: ChatPanelPosition::default(),
            chat_temperature: default_chat_temperature(),
            chat_max_tool_iterations: default_chat_max_tool_iterations(),
            chat_max_tokens: default_chat_max_tokens(),
            chat_max_tokens_unlimited: false,
            chat_export_dir: default_chat_export_dir(),
            chat_audit_log_enabled: false,
            chat_audit_log_warn_bytes: default_chat_audit_warn_bytes(),
            chat_audit_log_warn_enabled: true,
            chat_api_keys: std::collections::BTreeMap::new(),
            summary_stats: default_summary_stats(),
            directory_tree_filter_enabled: true,
        }
    }
}

impl AppSettings {
    /// Platform-specific config directory.
    pub fn config_dir() -> Option<PathBuf> {
        #[cfg(target_os = "linux")]
        {
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .ok()
                .or_else(|| dirs_path_home().map(|h| h.join(".config")))
                .map(|d| d.join("octa"))
        }
        #[cfg(target_os = "windows")]
        {
            std::env::var("APPDATA")
                .map(PathBuf::from)
                .ok()
                .map(|d| d.join("Octa"))
        }
        #[cfg(target_os = "macos")]
        {
            dirs_path_home().map(|h| h.join("Library/Application Support/Octa"))
        }
    }

    fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("settings.toml"))
    }

    /// Load settings from disk, falling back to defaults.
    ///
    /// Robustness: missing/extra fields are tolerated via `#[serde(default)]`
    /// at the struct level. Hard parse failures (e.g. an enum variant the
    /// current binary no longer knows) cause the broken file to be copied
    /// alongside as `settings.toml.bak-<unix-timestamp>` before defaults are
    /// returned, so the user can recover their values manually.
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let contents = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return Self::default(),
        };
        match toml::from_str::<Self>(&contents) {
            Ok(s) => s,
            Err(err) => {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let backup = path.with_file_name(format!("settings.toml.bak-{ts}"));
                let _ = std::fs::copy(&path, &backup);
                eprintln!(
                    "octa: failed to parse {} ({err}); backed up to {} and using defaults.",
                    path.display(),
                    backup.display(),
                );
                Self::default()
            }
        }
    }

    /// Persist settings to disk. The file may carry plaintext API keys (the
    /// keyring fallback), so it is restricted to the owning user.
    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Narrow the window: clamp an existing file before rewriting it.
            if path.exists() {
                restrict_file_to_owner(&path);
            }
            if let Ok(contents) = toml::to_string_pretty(self) {
                let _ = std::fs::write(&path, contents);
                restrict_file_to_owner(&path);
            }
        }
    }
}

/// Best-effort chmod 0600: only the owning user may read files that can hold
/// secrets (plaintext API keys in `settings.toml`, chat transcripts). No-op on
/// non-Unix platforms, where ACLs already scope `%APPDATA%` to the user.
pub fn restrict_file_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Best-effort chmod 0700 for directories holding sensitive files. See
/// [`restrict_file_to_owner`].
pub fn restrict_dir_to_owner(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Helper: get the user's home directory without pulling in the `dirs` crate.
fn dirs_path_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME").map(PathBuf::from).ok()
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").map(PathBuf::from).ok()
    }
}

/// Transient state for the settings dialog.
#[derive(Default)]
pub struct SettingsDialog {
    pub open: bool,
    /// Working copy - committed on Apply/OK.
    pub draft: AppSettings,
    /// Whether the icon changed (needs texture + window icon refresh).
    pub icon_changed: bool,
    /// Whether font size changed (needs style reapply).
    pub font_changed: bool,
    /// Whether theme changed.
    pub theme_changed: bool,
    /// Buffer backing the SQL row-limit text input. Parsed into the draft
    /// on Apply so the user can type freely without drag widgets fighting them.
    sql_row_limit_buf: String,
    /// Buffer backing the syntax-highlight size text input. Holds the value
    /// in whichever unit `syntax_highlight_size_unit` currently picks, with
    /// comma thousand separators so it matches Octa's display conventions.
    /// Parsed on Apply.
    syntax_highlight_max_bytes_buf: String,
    /// Display unit for the syntax-highlight size input. Not persisted -
    /// reset each time the dialog opens.
    syntax_highlight_size_unit: SyntaxSizeUnit,
    /// Buffer backing the initial-load-rows text input. Holds a comma-
    /// separated integer (e.g. "1,000,000"). Parsed on Apply.
    initial_load_rows_buf: String,
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
    /// Buffer backing the chat temperature input (text, not a slider, so
    /// Settings shows no drag cursor). Parsed + clamped 0.0..=2.0 on Apply.
    chat_temperature_buf: String,
    /// Buffer backing the chat max-tool-iterations input. Parsed + clamped
    /// 1..=30 on Apply.
    chat_max_iterations_buf: String,
    /// Buffer backing the chat max-response-tokens input. Comma-tolerant
    /// integer; ignored when `chat_unlimited_tokens` is checked. Parsed on Apply.
    chat_max_tokens_buf: String,
    /// Mirrors `AppSettings.chat_max_tokens_unlimited`: when true the cap input
    /// is greyed out and Apply writes the unlimited flag.
    chat_unlimited_tokens: bool,
    /// Audit-log size-warning threshold in MB (text buffer; parsed on Apply
    /// into `chat_audit_log_warn_bytes`).
    chat_audit_warn_mb_buf: String,
    /// Masked API-key entry buffer for the chat provider, in the Chat section.
    chat_key_input_buf: String,
    /// Last "where the key was stored" status line after a Save/Clear.
    chat_key_status_msg: Option<String>,
    /// Set when the user clicks "Clear" on a chat API key: holds the provider
    /// awaiting deletion confirmation. `None` = no pending confirmation. Guards
    /// against an accidental one-click key wipe.
    chat_key_clear_confirm: Option<ChatProviderKind>,
    /// Set by the chat panel's Settings button so the Chat section opens
    /// expanded; consumed (reset) once the dialog has honoured it.
    pub focus_chat_section: bool,
    /// When the user clicks "Record" for a shortcut, the action is stored here
    /// and the next key press captures a new binding. `None` = not recording.
    recording: Option<ShortcutAction>,
    /// Set when the user tries to bind a combo that is already used by another
    /// action. Cleared when they record successfully or edit the grid again.
    shortcut_conflict: Option<String>,
    /// Whether the "Reset to defaults" confirmation modal is currently shown.
    show_reset_confirm: bool,
    /// Window-size mode for the dialog (Normal / Maximized / Minimized).
    /// Persists across re-opens within the same app session - closing and
    /// reopening Settings keeps the size choice the user last picked.
    size: DialogSize,
}

/// Window-size mode for a dialog. `Maximized` forces a full-screen rect;
/// `Minimized` hides the body so only the header bar is shown (the checkbox
/// stays visible there to restore). `Normal` is the default size.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum DialogSize {
    #[default]
    Normal,
    Maximized,
    Minimized,
}

/// Render the three title-bar control buttons (Minimize, Maximize, Close)
/// into the current `ui` in right-to-left order (so the visual order is
/// `[_] [□] [x]`, matching desktop convention). Updates `*size` per click,
/// with mutual exclusion between Minimize and Maximize. Returns `true` when
/// the user clicked the close button.
///
/// Glyph choice: stick to characters the egui default font definitely
/// renders - underscore, U+25A1 white square, and `x`. Trying ─ / ⛶ / ✕
/// silently falls back to a missing-glyph box so all three buttons end up
/// visually identical.
pub fn draw_window_controls(ui: &mut egui::Ui, size: &mut DialogSize) -> bool {
    let btn_size = egui::vec2(26.0, 22.0);
    let mut close = false;

    // Close - bold lowercase `x`.
    if ui
        .add(egui::Button::new(egui::RichText::new("x").size(15.0).strong()).min_size(btn_size))
        .on_hover_text(crate::i18n::t("settings_hint.ctrl_close"))
        .clicked()
    {
        close = true;
    }
    // Maximize - U+25A1 WHITE SQUARE. `selected(active)` highlights it.
    let max_active = *size == DialogSize::Maximized;
    if ui
        .add(
            egui::Button::new(egui::RichText::new("\u{25A1}").size(14.0))
                .selected(max_active)
                .min_size(btn_size),
        )
        .on_hover_text(if max_active {
            crate::i18n::t("settings_hint.ctrl_restore")
        } else {
            crate::i18n::t("settings_hint.ctrl_full_size")
        })
        .clicked()
    {
        *size = if max_active {
            DialogSize::Normal
        } else {
            DialogSize::Maximized
        };
    }
    // Minimize - plain ASCII underscore, lowered visually so it sits where
    // the Windows minimize bar sits (the underscore baseline draws low,
    // matching the convention).
    let min_active = *size == DialogSize::Minimized;
    if ui
        .add(
            egui::Button::new(egui::RichText::new("_").size(15.0).strong())
                .selected(min_active)
                .min_size(btn_size),
        )
        .on_hover_text(if min_active {
            crate::i18n::t("settings_hint.ctrl_restore")
        } else {
            crate::i18n::t("settings_hint.ctrl_minimise")
        })
        .clicked()
    {
        *size = if min_active {
            DialogSize::Normal
        } else {
            DialogSize::Minimized
        };
    }
    close
}

/// Configure a dialog's `egui::Window` for the given [`DialogSize`], restoring
/// the pre-maximize size when the user un-maximizes.
///
/// egui persists a window's rect by id, so once a dialog is shown with
/// `fixed_rect(full_screen)` (Maximized) the stored rect stays full size and a
/// plain switch back to a resizable builder leaves the window stuck large -
/// the user "can't reduce it". This forces the remembered pre-maximize rect
/// for the single frame after un-maximizing, which overwrites egui's stored
/// rect; the window is resizable again from the next frame.
///
/// `id` must be stable per dialog and match the value passed to
/// [`remember_dialog_rect`]. `normal` applies the dialog's own Normal-size
/// builder settings (resizable, min/default size, default pos).
pub fn size_dialog_window<'a>(
    ctx: &egui::Context,
    id: egui::Id,
    size: DialogSize,
    window: egui::Window<'a>,
    normal: impl FnOnce(egui::Window<'a>) -> egui::Window<'a>,
) -> egui::Window<'a> {
    let prev_key = id.with("octa_dlg_prev_size");
    let rect_key = id.with("octa_dlg_normal_rect");
    // Track the previous frame's size so we can detect the Maximized -> Normal
    // transition that needs the one-frame restore.
    let prev = ctx.data_mut(|d| {
        let p = d.get_temp::<DialogSize>(prev_key);
        d.insert_temp(prev_key, size);
        p
    });
    match size {
        DialogSize::Maximized => window.fixed_rect(ctx.content_rect().shrink(8.0)),
        DialogSize::Minimized => window.resizable(false),
        DialogSize::Normal => {
            if prev == Some(DialogSize::Maximized)
                && let Some(rect) = ctx.data(|d| d.get_temp::<egui::Rect>(rect_key))
            {
                normal(window).fixed_rect(rect)
            } else {
                normal(window)
            }
        }
    }
}

/// Record a dialog window's current rect so a later un-maximize can restore it.
/// Call after `Window::show` with the inner response's rect. Only the Normal
/// mode's rect is remembered (the Maximized/Minimized rects are derived).
pub fn remember_dialog_rect(ctx: &egui::Context, id: egui::Id, size: DialogSize, rect: egui::Rect) {
    if size == DialogSize::Normal {
        ctx.data_mut(|d| d.insert_temp(id.with("octa_dlg_normal_rect"), rect));
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
