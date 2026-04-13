use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::theme::ThemeMode;
use crate::data::SearchMode;

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

/// Available icon color variants (matching assets/octa-*.svg files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconVariant {
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
    pub fn svg_source(self) -> &'static str {
        match self {
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

    /// Preview color for the icon picker UI.
    pub fn preview_color(self) -> egui::Color32 {
        use egui::Color32;
        match self {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size: 13.0,
            default_theme: ThemeMode::Light,
            icon_variant: IconVariant::Rose,
            default_search_mode: SearchMode::Plain,
            show_row_numbers: true,
            alternating_row_colors: true,
            negative_numbers_red: false,
            highlight_edits: false,
            color_aligned_columns: true,
            notebook_output_layout: NotebookOutputLayout::default(),
        }
    }
}

impl AppSettings {
    /// Platform-specific config directory.
    fn config_dir() -> Option<PathBuf> {
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
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
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
}

impl SettingsDialog {
    /// Open the dialog, seeding the draft from current settings.
    pub fn open(&mut self, current: &AppSettings) {
        self.draft = current.clone();
        self.icon_changed = false;
        self.font_changed = false;
        self.theme_changed = false;
        self.open = true;
    }

    /// Draw the dialog. Returns `Some(settings)` when the user clicks Apply.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<AppSettings> {
        if !self.open {
            return None;
        }

        let mut applied: Option<AppSettings> = None;

        egui::Window::new("Settings")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .min_width(360.0)
            .show(ctx, |ui| {
                egui::Grid::new("settings_grid")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        // --- Font size ---
                        ui.label("Font size:");
                        let old_size = self.draft.font_size;
                        ui.add(
                            egui::DragValue::new(&mut self.draft.font_size)
                                .range(8.0..=32.0)
                                .speed(0.25)
                                .suffix(" pt"),
                        );
                        if self.draft.font_size != old_size {
                            self.font_changed = true;
                        }
                        ui.end_row();

                        // --- Default theme ---
                        ui.label("Default theme:");
                        let old_theme = self.draft.default_theme;
                        egui::ComboBox::from_id_salt("theme_combo")
                            .selected_text(self.draft.default_theme.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.draft.default_theme,
                                    ThemeMode::Light,
                                    "Light",
                                );
                                ui.selectable_value(
                                    &mut self.draft.default_theme,
                                    ThemeMode::Dark,
                                    "Dark",
                                );
                            });
                        if self.draft.default_theme != old_theme {
                            self.theme_changed = true;
                        }
                        ui.end_row();

                        // --- Icon variant ---
                        ui.label("Icon color:");
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

                        // --- Default search mode ---
                        ui.label("Default search:");
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

                        // --- Show row numbers ---
                        ui.label("Show row numbers:");
                        ui.checkbox(&mut self.draft.show_row_numbers, "");
                        ui.end_row();

                        // --- Alternating row colors ---
                        ui.label("Alternating row colors:");
                        ui.checkbox(&mut self.draft.alternating_row_colors, "");
                        ui.end_row();

                        // --- Negative numbers in red ---
                        ui.label("Negative numbers in red:");
                        ui.checkbox(&mut self.draft.negative_numbers_red, "");
                        ui.end_row();

                        // --- Highlight edited cells ---
                        ui.label("Highlight edited cells:");
                        ui.checkbox(&mut self.draft.highlight_edits, "");
                        ui.end_row();

                        // --- Color aligned columns ---
                        ui.label("Color aligned columns:");
                        ui.checkbox(&mut self.draft.color_aligned_columns, "");
                        ui.end_row();

                        // --- Notebook output layout ---
                        ui.label("Notebook output:");
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

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        applied = Some(self.draft.clone());
                        self.open = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });

        applied
    }
}
