use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::shortcuts::{KeyCombo, ShortcutAction, Shortcuts};
use super::theme::{BodyFont, ThemeMode};
use crate::data::{BinaryDisplayMode, MarkColor, SearchMode};

/// Layout for Jupyter notebook output cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotebookOutputLayout {
    /// Output shown beside the source cell (side by side).
    #[default]
    Beside,
    /// Output shown beneath the source cell (like Jupyter).
    Beneath,
}

impl NotebookOutputLayout {
    pub fn label(self) -> &'static str {
        match self {
            Self::Beside => "Beside",
            Self::Beneath => "Beneath",
        }
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
    ];

    /// All concrete (non-Random) variants — what `Random` rolls between.
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
        }
    }

    /// Returns the SVG source for this icon variant (compile-time embedded).
    /// For `Random`, returns a multi-color rosette used only as a preview.
    /// Callers that render the actual app icon must call [`Self::resolve`] first.
    pub fn svg_source(self) -> &'static str {
        match self {
            Self::Random => include_str!("../../assets/octa-random.svg"),
            Self::Rose => include_str!("../../assets/octa-rose.svg"),
            Self::Amber => include_str!("../../assets/octa-amber.svg"),
            Self::Blue => include_str!("../../assets/octa-blue.svg"),
            Self::Cyan => include_str!("../../assets/octa-cyan.svg"),
            Self::Emerald => include_str!("../../assets/octa-emerald.svg"),
            Self::Indigo => include_str!("../../assets/octa-indigo.svg"),
            Self::Lime => include_str!("../../assets/octa-lime.svg"),
            Self::Orange => include_str!("../../assets/octa-orange.svg"),
            Self::Purple => include_str!("../../assets/octa-purple.svg"),
            Self::Red => include_str!("../../assets/octa-red.svg"),
            Self::Slate => include_str!("../../assets/octa-slate.svg"),
            Self::Teal => include_str!("../../assets/octa-teal.svg"),
        }
    }

    /// Resolve a concrete variant: returns `self` for any concrete variant; for
    /// `Random`, picks one of [`Self::CONCRETE`] uniformly at random.
    pub fn resolve(self) -> IconVariant {
        use rand::seq::SliceRandom;
        if self == Self::Random {
            *Self::CONCRETE
                .choose(&mut rand::thread_rng())
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
        }
    }
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
    /// Whether to use alternating row background colors.
    #[serde(default = "default_true")]
    pub alternating_row_colors: bool,
    /// Whether negative numbers are displayed in red.
    #[serde(default)]
    pub negative_numbers_red: bool,
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
    /// Where to dock the directory tree sidebar when a folder is open.
    #[serde(default)]
    pub directory_tree_position: DirectoryTreePosition,
    /// Whether to show a confirmation warning before toggling "Align Columns"
    /// off in the raw CSV/TSV view, which reloads the file and discards edits.
    #[serde(default = "default_true")]
    pub warn_raw_align_reload: bool,
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
}

fn default_true() -> bool {
    true
}

fn default_max_recent() -> usize {
    5
}

fn default_tab_size() -> usize {
    4
}

fn default_sql_row_limit() -> usize {
    100
}

fn default_mark_color() -> MarkColor {
    MarkColor::Yellow
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            default_theme: ThemeMode::Light,
            icon_variant: IconVariant::Random,
            default_search_mode: SearchMode::Plain,
            show_row_numbers: true,
            alternating_row_colors: true,
            negative_numbers_red: false,
            highlight_edits: false,
            cell_line_breaks: false,
            binary_display_mode: BinaryDisplayMode::default(),
            color_aligned_columns: true,
            notebook_output_layout: NotebookOutputLayout::default(),
            max_recent_files: 5,
            tab_size: 4,
            body_font: BodyFont::Proportional,
            custom_font_path: String::new(),
            default_mark_color: default_mark_color(),
            sql_panel_default_open: false,
            sql_panel_position: SqlPanelPosition::default(),
            sql_default_row_limit: 100,
            sql_autocomplete: true,
            directory_tree_position: DirectoryTreePosition::default(),
            warn_raw_align_reload: true,
            shortcuts: Shortcuts::default(),
            window_size: WindowSize::default(),
            start_maximized: true,
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

    /// Persist settings to disk.
    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(contents) = toml::to_string_pretty(self) {
                let _ = std::fs::write(path, contents);
            }
        }
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
    /// Working copy — committed on Apply/OK.
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
    /// When the user clicks "Record" for a shortcut, the action is stored here
    /// and the next key press captures a new binding. `None` = not recording.
    recording: Option<ShortcutAction>,
    /// Set when the user tries to bind a combo that is already used by another
    /// action. Cleared when they record successfully or edit the grid again.
    shortcut_conflict: Option<String>,
    /// Whether the "Reset to defaults" confirmation modal is currently shown.
    show_reset_confirm: bool,
    /// Window-size mode for the dialog (Normal / Maximized / Minimized).
    /// Persists across re-opens within the same app session — closing and
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

impl SettingsDialog {
    /// Open the dialog, seeding the draft from current settings.
    pub fn open(&mut self, current: &AppSettings) {
        self.draft = current.clone();
        self.icon_changed = false;
        self.font_changed = false;
        self.theme_changed = false;
        self.sql_row_limit_buf = current.sql_default_row_limit.to_string();
        self.recording = None;
        self.shortcut_conflict = None;
        self.show_reset_confirm = false;
        self.open = true;
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

        // Custom title bar (egui's is disabled below) — we render Min /
        // Max / Close buttons inline next to the title, like a typical
        // desktop window. Dragging works because the title text is a
        // non-interactive area inside the window's drag region.
        let screen_center = ctx.screen_rect().center();
        let default_pos = screen_center - egui::vec2(240.0, 290.0);
        let mut window = egui::Window::new("Settings")
            .title_bar(false)
            .collapsible(false);
        window = match self.size {
            DialogSize::Maximized => window.fixed_rect(ctx.screen_rect().shrink(8.0)),
            // Minimized: no min sizing — let egui auto-shrink to the header.
            DialogSize::Minimized => window.resizable(false).default_pos(default_pos),
            DialogSize::Normal => window
                .resizable(true)
                .default_pos(default_pos)
                .min_width(460.0)
                .default_width(480.0)
                .default_height(580.0)
                .min_height(360.0),
        };
        let minimized = self.size == DialogSize::Minimized;
        window
            .show(ctx, |ui| {
                // Custom title bar: logo + "Octa Settings" + three control
                // buttons. Stays rendered when minimized so the user can
                // restore from there.
                egui::TopBottomPanel::top("settings_header")
                    .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(tex) = logo {
                                let size = egui::vec2(28.0, 28.0);
                                ui.add(egui::Image::new(tex).fit_to_exact_size(size));
                                ui.add_space(8.0);
                            }
                            ui.label(egui::RichText::new("Octa Settings").strong().size(16.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if draw_window_controls(ui, &mut self.size) {
                                        self.open = false;
                                    }
                                },
                            );
                        });
                    });

                if minimized {
                    return;
                }

                // Pin Apply/Cancel to the bottom so they're always reachable
                // regardless of how much content the scroll area holds.
                egui::TopBottomPanel::bottom("settings_buttons")
                    .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                if let Ok(n) = self.sql_row_limit_buf.trim().parse::<usize>() {
                                    if n >= 1 {
                                        self.draft.sql_default_row_limit = n;
                                    }
                                }
                                applied = Some(self.draft.clone());
                                self.open = false;
                            }
                            if ui.button("Cancel").clicked() {
                                self.open = false;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let label = egui::RichText::new("Reset to defaults")
                                        .color(ui.visuals().error_fg_color);
                                    if ui.button(label).clicked() {
                                        self.show_reset_confirm = true;
                                    }
                                },
                            );
                        });
                    });

                egui::CentralPanel::default()
                    .frame(egui::Frame::default())
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                self.draw_sections(ui);
                            });
                    });
            });

        applied
    }

    /// Render the "Reset to defaults?" confirmation modal. On confirm, the
    /// draft is replaced with `AppSettings::default()` and the icon/font/theme
    /// changed flags are set so the existing Apply path re-applies them.
    /// Nothing is written to disk and the Settings window stays open — the
    /// user still has to click Apply (or Cancel) to commit / discard.
    fn draw_reset_confirm(&mut self, ctx: &egui::Context) {
        if !self.show_reset_confirm {
            return;
        }
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Reset all settings to defaults?")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    "This replaces every value in the Settings dialog with its default.\n\
                     Nothing is saved until you click Apply — Cancel still reverts.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Reset").clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            self.draft = AppSettings::default();
            self.sql_row_limit_buf = self.draft.sql_default_row_limit.to_string();
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
        egui::CollapsingHeader::new(egui::RichText::new("Appearance").strong().size(13.0))
            .id_salt("settings_section_appearance")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_appearance")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Font size:")
                            .on_hover_text("Base font size for all text in the application");
                        let old_size = self.draft.font_size;
                        let current_pt = self.draft.font_size.round() as i32;
                        egui::ComboBox::from_id_salt("font_size_combo")
                            .selected_text(format!("{} pt", current_pt))
                            .show_ui(ui, |ui| {
                                for sz in 8..=32 {
                                    ui.selectable_value(
                                        &mut self.draft.font_size,
                                        sz as f32,
                                        format!("{} pt", sz),
                                    );
                                }
                            });
                        if self.draft.font_size != old_size {
                            self.font_changed = true;
                        }
                        ui.end_row();

                        ui.label("Default theme:")
                            .on_hover_text("Theme applied when the application starts");
                        let old_theme = self.draft.default_theme;
                        egui::ComboBox::from_id_salt("theme_combo")
                            .selected_text(self.draft.default_theme.label())
                            .show_ui(ui, |ui| {
                                for &preset in ThemeMode::ALL {
                                    ui.selectable_value(
                                        &mut self.draft.default_theme,
                                        preset,
                                        preset.label(),
                                    );
                                }
                            });
                        if self.draft.default_theme != old_theme {
                            self.theme_changed = true;
                        }
                        ui.end_row();

                        ui.label("Body font:").on_hover_text(
                            "Font family used for body, button and heading text.\n\
                                 Monospace gives every character the same width.",
                        );
                        let old_body_font = self.draft.body_font;
                        egui::ComboBox::from_id_salt("body_font_combo")
                            .selected_text(self.draft.body_font.label())
                            .show_ui(ui, |ui| {
                                for &choice in BodyFont::ALL {
                                    ui.selectable_value(
                                        &mut self.draft.body_font,
                                        choice,
                                        choice.label(),
                                    );
                                }
                            });
                        if self.draft.body_font != old_body_font {
                            self.font_changed = true;
                        }
                        ui.end_row();

                        ui.label("Custom font (.ttf, .otf, .ttc):").on_hover_text(
                            "Optional path to a TrueType (.ttf), OpenType (.otf),\n\
                                 or TrueType Collection (.ttc) font file. When set and\n\
                                 readable, overrides the body font choice for proportional text.\n\
                                 WOFF/WOFF2 are not supported.",
                        );
                        let old_path = self.draft.custom_font_path.clone();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.draft.custom_font_path)
                                    .hint_text("(none — .ttf, .otf, or .ttc)")
                                    .desired_width(220.0),
                            );
                            if ui.button("Browse...").clicked() {
                                if let Some(p) = rfd::FileDialog::new()
                                    .add_filter("Font (.ttf, .otf, .ttc)", &["ttf", "otf", "ttc"])
                                    .pick_file()
                                {
                                    self.draft.custom_font_path = p.to_string_lossy().into_owned();
                                }
                            }
                            if !self.draft.custom_font_path.is_empty()
                                && ui.button("Clear").clicked()
                            {
                                self.draft.custom_font_path.clear();
                            }
                        });
                        if self.draft.custom_font_path != old_path {
                            self.font_changed = true;
                        }
                        ui.end_row();

                        ui.label("Icon color:")
                            .on_hover_text("Color variant for the application icon");
                        let old_icon = self.draft.icon_variant;
                        egui::ComboBox::from_id_salt("icon_combo")
                            .selected_text(self.draft.icon_variant.label())
                            .show_ui(ui, |ui| {
                                for &variant in IconVariant::ALL {
                                    let color = variant.preview_color();
                                    let text = egui::RichText::new(variant.label()).color(color);
                                    ui.selectable_value(
                                        &mut self.draft.icon_variant,
                                        variant,
                                        text,
                                    );
                                }
                            });
                        if self.draft.icon_variant != old_icon {
                            self.icon_changed = true;
                        }
                        ui.end_row();
                    });
            });

        // ── Table View ──
        egui::CollapsingHeader::new(egui::RichText::new("Table View").strong().size(13.0))
            .id_salt("settings_section_table")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_table")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Show row numbers:")
                            .on_hover_text("Display row numbers in the leftmost column");
                        ui.checkbox(&mut self.draft.show_row_numbers, "");
                        ui.end_row();

                        ui.label("Alternating row colors:")
                            .on_hover_text("Alternate row background colors for readability");
                        ui.checkbox(&mut self.draft.alternating_row_colors, "");
                        ui.end_row();

                        ui.label("Negative numbers in red:")
                            .on_hover_text("Highlight negative numeric values with red text");
                        ui.checkbox(&mut self.draft.negative_numbers_red, "");
                        ui.end_row();

                        ui.label("Highlight edited cells:")
                            .on_hover_text("Show background color on modified cells");
                        ui.checkbox(&mut self.draft.highlight_edits, "");
                        ui.end_row();

                        ui.label("Cell line breaks:").on_hover_text(
                            "Allow long text to wrap onto multiple lines\n\
                             within a table cell instead of clipping",
                        );
                        ui.checkbox(&mut self.draft.cell_line_breaks, "");
                        ui.end_row();

                        ui.label("Binary display:").on_hover_text(
                            "How to show binary data columns\n\
                             Binary: raw bits (01000001)\n\
                             Hex: hexadecimal (41)\n\
                             Text: decode as UTF-8 when possible",
                        );
                        egui::ComboBox::from_id_salt("binary_display_combo")
                            .selected_text(self.draft.binary_display_mode.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.draft.binary_display_mode,
                                    BinaryDisplayMode::Binary,
                                    BinaryDisplayMode::Binary.label(),
                                );
                                ui.selectable_value(
                                    &mut self.draft.binary_display_mode,
                                    BinaryDisplayMode::Hex,
                                    BinaryDisplayMode::Hex.label(),
                                );
                                ui.selectable_value(
                                    &mut self.draft.binary_display_mode,
                                    BinaryDisplayMode::Text,
                                    BinaryDisplayMode::Text.label(),
                                );
                            });
                        ui.end_row();

                        ui.label("Default mark color:").on_hover_text(
                            "Color applied by the Mark shortcut (Ctrl+M by default).\n\
                             The toolbar / context menu still let you pick any color.",
                        );
                        egui::ComboBox::from_id_salt("default_mark_color_combo")
                            .selected_text(self.draft.default_mark_color.label())
                            .show_ui(ui, |ui| {
                                for &color in MarkColor::ALL {
                                    ui.selectable_value(
                                        &mut self.draft.default_mark_color,
                                        color,
                                        color.label(),
                                    );
                                }
                            });
                        ui.end_row();
                    });
            });

        // ── Search & Editor ──
        egui::CollapsingHeader::new(egui::RichText::new("Search & Editor").strong().size(13.0))
            .id_salt("settings_section_search_editor")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_search_editor")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Default search mode:")
                            .on_hover_text("Default search/filter mode for new tabs");
                        egui::ComboBox::from_id_salt("search_mode_combo")
                            .selected_text(self.draft.default_search_mode.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.draft.default_search_mode,
                                    SearchMode::Plain,
                                    "Plain",
                                );
                                ui.selectable_value(
                                    &mut self.draft.default_search_mode,
                                    SearchMode::Wildcard,
                                    "Wildcard",
                                );
                                ui.selectable_value(
                                    &mut self.draft.default_search_mode,
                                    SearchMode::Regex,
                                    "Regex",
                                );
                            });
                        ui.end_row();

                        ui.label("Tab size:")
                            .on_hover_text("Spaces inserted when pressing Tab in the text editor");
                        egui::ComboBox::from_id_salt("tab_size_combo")
                            .selected_text(self.draft.tab_size.to_string())
                            .width(40.0)
                            .show_ui(ui, |ui| {
                                for n in 1..=16 {
                                    ui.selectable_value(&mut self.draft.tab_size, n, n.to_string());
                                }
                            });
                        ui.end_row();
                    });
            });

        // ── File-Specific ──
        egui::CollapsingHeader::new(egui::RichText::new("File-Specific").strong().size(13.0))
            .id_salt("settings_section_format")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_format")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Color aligned columns:").on_hover_text(
                            "Color columns in CSV/TSV Raw Text view\n\
                             (only applies when 'Align Columns' is enabled)",
                        );
                        ui.checkbox(&mut self.draft.color_aligned_columns, "");
                        ui.end_row();

                        ui.label("Warn before un-aligning CSV:").on_hover_text(
                            "Confirm before turning 'Align Columns' off in the raw\n\
                             CSV/TSV view — un-aligning reloads the file from disk\n\
                             and discards in-buffer edits.",
                        );
                        ui.checkbox(&mut self.draft.warn_raw_align_reload, "");
                        ui.end_row();

                        ui.label("Notebook output:").on_hover_text(
                            "Code output position in Jupyter Notebook view\n\
                             (only applies to .ipynb files in Notebook view mode)",
                        );
                        egui::ComboBox::from_id_salt("notebook_layout_combo")
                            .selected_text(self.draft.notebook_output_layout.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.draft.notebook_output_layout,
                                    NotebookOutputLayout::Beside,
                                    "Beside",
                                );
                                ui.selectable_value(
                                    &mut self.draft.notebook_output_layout,
                                    NotebookOutputLayout::Beneath,
                                    "Beneath",
                                );
                            });
                        ui.end_row();
                    });
            });

        // ── SQL ──
        egui::CollapsingHeader::new(egui::RichText::new("SQL").strong().size(13.0))
            .id_salt("settings_section_sql")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_sql")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Open SQL panel by default:").on_hover_text(
                            "When opening a tabular file, automatically show the\n\
                             SQL editor alongside the table view.",
                        );
                        ui.checkbox(&mut self.draft.sql_panel_default_open, "");
                        ui.end_row();

                        ui.label("SQL panel position:")
                            .on_hover_text("Where the SQL editor docks relative to the table");
                        egui::ComboBox::from_id_salt("sql_panel_position_combo")
                            .selected_text(self.draft.sql_panel_position.label())
                            .show_ui(ui, |ui| {
                                for &pos in SqlPanelPosition::ALL {
                                    ui.selectable_value(
                                        &mut self.draft.sql_panel_position,
                                        pos,
                                        pos.label(),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label("Default row limit:").on_hover_text(
                            "LIMIT used in the placeholder query when a tab is opened\n\
                             (e.g. 100 → SELECT * FROM data LIMIT 100).\n\
                             Type a number — applied on Apply.",
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.sql_row_limit_buf)
                                .desired_width(80.0)
                                .hint_text("100"),
                        );
                        ui.end_row();

                        ui.label("Autocomplete:").on_hover_text(
                            "Offer SQL keyword and column-name suggestions\n\
                             beneath the SQL editor.",
                        );
                        ui.checkbox(&mut self.draft.sql_autocomplete, "");
                        ui.end_row();
                    });
            });

        // ── Directory Tree ──
        egui::CollapsingHeader::new(egui::RichText::new("Directory Tree").strong().size(13.0))
            .id_salt("settings_section_directory_tree")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_directory_tree")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Sidebar position:").on_hover_text(
                            "Which side the directory tree is docked on when a\n\
                         folder is opened via File > Open Directory.",
                        );
                        egui::ComboBox::from_id_salt("directory_tree_position_combo")
                            .selected_text(self.draft.directory_tree_position.label())
                            .show_ui(ui, |ui| {
                                for &pos in DirectoryTreePosition::ALL {
                                    ui.selectable_value(
                                        &mut self.draft.directory_tree_position,
                                        pos,
                                        pos.label(),
                                    );
                                }
                            });
                        ui.end_row();
                    });
            });

        // ── Shortcuts ──
        egui::CollapsingHeader::new(egui::RichText::new("Shortcuts").strong().size(13.0))
            .id_salt("settings_section_shortcuts")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Click 'Record' then press a key combo (with Ctrl / Shift / Alt).\n\
                         Press Esc to cancel recording. 'Clear' leaves the action unbound.",
                    )
                    .weak()
                    .size(11.0),
                );
                ui.add_space(6.0);
                self.draw_shortcuts_grid(ui);
            });

        // ── Files ──
        egui::CollapsingHeader::new(egui::RichText::new("Files").strong().size(13.0))
            .id_salt("settings_section_files")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_files")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Max recent files:").on_hover_text(
                            "Number of recently opened files shown in the File menu",
                        );
                        egui::ComboBox::from_id_salt("max_recent_combo")
                            .selected_text(self.draft.max_recent_files.to_string())
                            .width(50.0)
                            .show_ui(ui, |ui| {
                                for n in 1..=30 {
                                    ui.selectable_value(
                                        &mut self.draft.max_recent_files,
                                        n,
                                        n.to_string(),
                                    );
                                }
                            });
                        ui.end_row();
                    });
            });

        // ── Window ──
        egui::CollapsingHeader::new(egui::RichText::new("Window").strong().size(13.0))
            .id_salt("settings_section_window")
            .default_open(false)
            .show(ui, |ui| {
                egui::Grid::new("settings_window")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Start maximized:").on_hover_text(
                            "When on, the window launches maximized and the size below\n\
                             is used as the restore-from-maximize size.\n\
                             When off, the window launches at the chosen size.",
                        );
                        ui.checkbox(&mut self.draft.start_maximized, "");
                        ui.end_row();

                        ui.label("Initial window size:").on_hover_text(
                            "Window size used at startup (when \"Start maximized\" is off),\n\
                             or the restore-from-maximize size when it is on.",
                        );
                        ui.add_enabled_ui(!self.draft.start_maximized, |ui| {
                            egui::ComboBox::from_id_salt("window_size_combo")
                                .selected_text(self.draft.window_size.label())
                                .show_ui(ui, |ui| {
                                    for &size in WindowSize::ALL {
                                        ui.selectable_value(
                                            &mut self.draft.window_size,
                                            size,
                                            size.label(),
                                        );
                                    }
                                });
                        });
                        ui.end_row();
                    });
            });
    }

    /// One grid row per [`ShortcutAction`]: name, current combo, Record/Clear/Reset.
    fn draw_shortcuts_grid(&mut self, ui: &mut egui::Ui) {
        use strum::IntoEnumIterator;
        // If the user is recording a binding, capture the next real key press.
        if let Some(action) = self.recording {
            let captured = ui.input(capture_combo);
            if let Some(CaptureResult::Cancel) = captured {
                self.recording = None;
            } else if let Some(CaptureResult::Combo(combo)) = captured {
                // Reject combos already bound to another action so two
                // functions can never share a shortcut.
                let conflict = self
                    .draft
                    .shortcuts
                    .bindings
                    .iter()
                    .find(|(other, existing)| **other != action && **existing == combo)
                    .map(|(other, _)| *other);
                if let Some(other) = conflict {
                    self.shortcut_conflict = Some(format!(
                        "{} is already bound to \"{}\". Clear that binding first or pick a different key.",
                        combo.label(),
                        other.label(),
                    ));
                } else {
                    self.draft.shortcuts.set(action, combo);
                    self.shortcut_conflict = None;
                }
                self.recording = None;
            }
        }

        if let Some(msg) = &self.shortcut_conflict {
            ui.colored_label(egui::Color32::from_rgb(0xd9, 0x53, 0x4f), msg);
            ui.add_space(4.0);
        }

        egui::Grid::new("settings_shortcuts_grid")
            .num_columns(4)
            .spacing([12.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for action in ShortcutAction::iter() {
                    ui.label(action.label());
                    let combo = self.draft.shortcuts.combo(action);
                    let label_text = if self.recording == Some(action) {
                        egui::RichText::new("Press any key…").italics()
                    } else {
                        egui::RichText::new(combo.label()).monospace()
                    };
                    ui.label(label_text);
                    if self.recording == Some(action) {
                        if ui.button("Stop").clicked() {
                            self.recording = None;
                        }
                    } else if ui.button("Record").clicked() {
                        self.recording = Some(action);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Clear").clicked() {
                            self.draft.shortcuts.set(action, KeyCombo::UNBOUND);
                        }
                        if ui.button("Reset").clicked() {
                            self.draft.shortcuts.reset(action);
                        }
                    });
                    ui.end_row();
                }
            });
    }
}

/// Render the three title-bar control buttons (Minimize, Maximize, Close)
/// into the current `ui` in right-to-left order (so the visual order is
/// `[_] [□] [x]`, matching desktop convention). Updates `*size` per click,
/// with mutual exclusion between Minimize and Maximize. Returns `true` when
/// the user clicked the close button.
///
/// Glyph choice: stick to characters the egui default font definitely
/// renders — underscore, U+25A1 white square, and `x`. Trying ─ / ⛶ / ✕
/// silently falls back to a missing-glyph box so all three buttons end up
/// visually identical.
pub fn draw_window_controls(ui: &mut egui::Ui, size: &mut DialogSize) -> bool {
    let btn_size = egui::vec2(26.0, 22.0);
    let mut close = false;

    // Close — bold lowercase `x`.
    if ui
        .add(
            egui::Button::new(egui::RichText::new("x").size(15.0).strong())
                .min_size(btn_size),
        )
        .on_hover_text("Close")
        .clicked()
    {
        close = true;
    }
    // Maximize — U+25A1 WHITE SQUARE. `selected(active)` highlights it.
    let max_active = *size == DialogSize::Maximized;
    if ui
        .add(
            egui::Button::new(egui::RichText::new("\u{25A1}").size(14.0))
                .selected(max_active)
                .min_size(btn_size),
        )
        .on_hover_text(if max_active { "Restore" } else { "Full size" })
        .clicked()
    {
        *size = if max_active { DialogSize::Normal } else { DialogSize::Maximized };
    }
    // Minimize — plain ASCII underscore, lowered visually so it sits where
    // the Windows minimize bar sits (the underscore baseline draws low,
    // matching the convention).
    let min_active = *size == DialogSize::Minimized;
    if ui
        .add(
            egui::Button::new(egui::RichText::new("_").size(15.0).strong())
                .selected(min_active)
                .min_size(btn_size),
        )
        .on_hover_text(if min_active { "Restore" } else { "Minimize" })
        .clicked()
    {
        *size = if min_active { DialogSize::Normal } else { DialogSize::Minimized };
    }
    close
}

/// Result of a single-frame shortcut capture.
enum CaptureResult {
    Cancel,
    Combo(KeyCombo),
}

/// While recording, watch for a non-modifier key press and return it with the
/// current modifier state. Esc cancels.
fn capture_combo(input: &egui::InputState) -> Option<CaptureResult> {
    if input.key_pressed(egui::Key::Escape) {
        return Some(CaptureResult::Cancel);
    }
    let mods = input.modifiers;
    for ev in &input.events {
        if let egui::Event::Key {
            key,
            pressed: true,
            repeat: false,
            ..
        } = ev
        {
            if matches!(key, egui::Key::Escape) {
                return Some(CaptureResult::Cancel);
            }
            return Some(CaptureResult::Combo(KeyCombo {
                key: Some(*key),
                ctrl: mods.command,
                shift: mods.shift,
                alt: mods.alt,
            }));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_toml_loads_with_defaults_for_missing_fields() {
        // Writing only `font_size` should still deserialize cleanly: every
        // other field is filled from `AppSettings::default()` thanks to the
        // struct-level `#[serde(default)]`. This is the upgrade-survivability
        // contract.
        let partial = "font_size = 10.0\n";
        let settings: AppSettings =
            toml::from_str(partial).expect("partial TOML must deserialize");
        let defaults = AppSettings::default();
        assert_eq!(settings.font_size, 10.0);
        assert_eq!(settings.default_theme, defaults.default_theme);
        assert_eq!(settings.icon_variant, defaults.icon_variant);
        assert_eq!(settings.show_row_numbers, defaults.show_row_numbers);
        assert_eq!(settings.sql_default_row_limit, defaults.sql_default_row_limit);
        assert_eq!(settings.start_maximized, defaults.start_maximized);
    }

    #[test]
    fn unknown_fields_are_silently_ignored() {
        // A field this binary doesn't know about (e.g. left over from a future
        // release downgraded back to the current one) must not blow up the
        // whole config — just skip it.
        let with_unknown = "font_size = 11.0\nmysterious_future_field = \"hi\"\n";
        let settings: AppSettings = toml::from_str(with_unknown)
            .expect("unknown fields should be tolerated");
        assert_eq!(settings.font_size, 11.0);
    }

    #[test]
    fn defaults_round_trip_through_toml() {
        let defaults = AppSettings::default();
        let serialized = toml::to_string_pretty(&defaults).expect("serialize");
        let parsed: AppSettings = toml::from_str(&serialized).expect("round-trip");
        assert_eq!(parsed.font_size, defaults.font_size);
        assert_eq!(parsed.default_theme, defaults.default_theme);
        assert_eq!(parsed.icon_variant, defaults.icon_variant);
        assert_eq!(parsed.start_maximized, defaults.start_maximized);
    }
}
