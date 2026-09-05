//! The presentation enums a setting can hold: panel positions, fonts,
//! provider kinds, size units, window size and the icon variant.
//!
//! Split out of `ui/settings/mod.rs`, which had grown to 2,150 lines holding
//! three unrelated things: the presentation enums, `AppSettings` itself, and
//! generic dialog chrome used by around seventy dialogs that have nothing to
//! do with settings. Code moved unchanged.

use serde::{Deserialize, Serialize};

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
    /// Docked above the main area (full width).
    Top,
    /// Docked below the main area (full width).
    Bottom,
}

impl DirectoryTreePosition {
    pub const ALL: &[DirectoryTreePosition] = &[Self::Left, Self::Right, Self::Top, Self::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Left => "enum.pos_left",
            Self::Right => "enum.pos_right",
            Self::Top => "enum.pos_top",
            Self::Bottom => "enum.pos_bottom",
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

    /// Where a user takes a complaint about generated content. Octa neither
    /// hosts nor trains a model, so content concerns belong with whoever
    /// serves it. `None` means there is no fixed page to link: Ollama runs on
    /// this machine and OpenAI-compatible is whatever URL the user typed, so
    /// the dialog names those instead. Reporting Octa itself is a separate
    /// button that is always present.
    ///
    /// Prefer a provider's dedicated reporting page over its general help
    /// centre. Where a site localises by visitor, link the locale-neutral
    /// path: Octa ships 32 languages, so a baked-in language segment would be
    /// wrong for 31 of them (openai.com resolves the locale itself). Anthropic
    /// keeps the `/en/` segment because that help centre is English-only and
    /// 301s the bare path there anyway, so dropping it only adds a hop.
    pub fn report_url(self) -> Option<&'static str> {
        match self {
            Self::Anthropic => Some(
                "https://support.claude.com/en/articles/\
                 7996906-reporting-blocking-and-removing-content-from-claude",
            ),
            Self::OpenAi => Some("https://openai.com/form/report-content/"),
            Self::Gemini => Some("https://support.google.com/gemini/"),
            Self::OpenAiCompatible | Self::Ollama => None,
        }
    }

    /// A sensible default model when the user has not picked one yet. Cheap
    /// models on purpose: a first chat should not surprise anyone on cost,
    /// and the dropdown makes the bigger models one click away.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-haiku-4-5-20251001",
            Self::OpenAi => "gpt-5.6-terra",
            Self::OpenAiCompatible => "deepseek/deepseek-v4-flash",
            Self::Gemini => "gemini-3.6-flash",
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
                "claude-opus-5",
                "claude-sonnet-5",
                "claude-fable-5",
                "claude-haiku-4-5-20251001",
                "claude-opus-4-8",
            ],
            Self::OpenAi => &["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"],
            Self::Gemini => &[
                "gemini-3.6-flash",
                "gemini-3.5-flash",
                "gemini-3.5-flash-lite",
                "gemini-3.1-pro-preview",
                "gemini-2.5-pro",
            ],
            // Open-weight models, named the way OpenRouter names them
            // (`vendor/model`), since that is the gateway most people point the
            // OpenAI-compatible provider at. Every other gateway spells the same
            // model differently, which is exactly why the field below the
            // dropdown stays free text. Ollama is dynamic (`/api/tags`).
            Self::OpenAiCompatible => &[
                "deepseek/deepseek-v4-pro",
                "deepseek/deepseek-v4-flash",
                "z-ai/glm-5.2",
                "moonshotai/kimi-k3",
                "moonshotai/kimi-k2.7-code",
                "qwen/qwen3.7-plus",
                "nvidia/nemotron-3-ultra-550b-a55b",
                "minimax/minimax-m3",
                "openai/gpt-oss-120b",
                "google/gemma-4-31b-it",
            ],
            Self::Ollama => &[],
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

/// Display unit for a byte-size setting in the Settings dialog.
/// Octa stores every such size as raw bytes in `settings.toml`; this enum only
/// governs how the value is presented and edited in the dialog. Not
/// persisted to the toml - defaults to MB at each open and the dialog
/// picks the most natural unit for the current value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeUnit {
    Bytes,
    KB,
    #[default]
    MB,
    GB,
}

impl SizeUnit {
    pub const ALL: &[SizeUnit] = &[Self::Bytes, Self::KB, Self::MB, Self::GB];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bytes => "Bytes",
            Self::KB => "KB",
            Self::MB => "MB",
            Self::GB => "GB",
        }
    }

    pub fn label_t(self) -> String {
        match self {
            Self::Bytes => crate::i18n::t("enum.unit_bytes"),
            Self::KB => "KB".to_string(),
            Self::MB => "MB".to_string(),
            Self::GB => "GB".to_string(),
        }
    }

    pub fn factor(self) -> usize {
        match self {
            Self::Bytes => 1,
            Self::KB => 1_024,
            Self::MB => 1_024 * 1_024,
            Self::GB => 1_024 * 1_024 * 1_024,
        }
    }

    /// Pick the largest unit that represents `bytes` as an integer
    /// (so 1,073,741,824 -> 1 GB; 1,048,576 -> 1 MB; 1,500 -> 1500 Bytes).
    pub fn best_fit(bytes: usize) -> Self {
        if bytes == 0 {
            return Self::MB;
        }
        if bytes.is_multiple_of(Self::GB.factor()) {
            return Self::GB;
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
    /// 400 × 300 - the window's minimum size (see `main.rs`), for parking Octa
    /// beside another window. Dragging the window small does not persist;
    /// picking it here does.
    W400x300,
    /// 640 × 480
    W640x480,
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
        Self::W400x300,
        Self::W640x480,
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
            Self::W400x300 => "400 × 300",
            Self::W640x480 => "640 × 480",
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
            Self::W400x300 => [400.0, 300.0],
            Self::W640x480 => [640.0, 480.0],
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
pub(crate) fn paint_icon_swatch(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
    ui.painter().rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Outside,
    );
}
