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
    /// Maximum number of recently opened files shown in the File menu.
    #[serde(default = "default_max_recent")]
    pub max_recent_files: usize,
    /// Number of spaces inserted when pressing Tab in the text editor.
    #[serde(default = "default_tab_size")]
    pub tab_size: usize,
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
            max_recent_files: 5,
            tab_size: 4,
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
            .min_width(420.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // ── Appearance ──
                    ui.label(egui::RichText::new("Appearance").strong().size(13.0));
                    ui.add_space(4.0);
                    egui::Grid::new("settings_appearance")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Font size:").on_hover_text(
                                "Base font size for all text in the application",
                            );
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

                            ui.label("Default theme:").on_hover_text(
                                "Theme applied when the application starts",
                            );
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

                            ui.label("Icon color:")
                                .on_hover_text("Color variant for the application icon");
                            let old_icon = self.draft.icon_variant;
                            egui::ComboBox::from_id_salt("icon_combo")
                                .selected_text(self.draft.icon_variant.label())
                                .show_ui(ui, |ui| {
                                    for &variant in IconVariant::ALL {
                                        let color = variant.preview_color();
                                        let text =
                                            egui::RichText::new(variant.label()).color(color);
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

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // ── Table View ──
                    ui.label(egui::RichText::new("Table View").strong().size(13.0));
                    ui.add_space(4.0);
                    egui::Grid::new("settings_table")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Show row numbers:")
                                .on_hover_text("Display row numbers in the leftmost column");
                            ui.checkbox(&mut self.draft.show_row_numbers, "");
                            ui.end_row();

                            ui.label("Alternating row colors:").on_hover_text(
                                "Alternate row background colors for readability",
                            );
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
                        });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // ── Search ──
                    ui.label(egui::RichText::new("Search").strong().size(13.0));
                    ui.add_space(4.0);
                    egui::Grid::new("settings_search")
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
                        });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // ── Editor ──
                    ui.label(egui::RichText::new("Editor").strong().size(13.0));
                    ui.add_space(4.0);
                    egui::Grid::new("settings_editor")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Tab size:");
                            egui::ComboBox::from_id_salt("tab_size_combo")
                                .selected_text(self.draft.tab_size.to_string())
                                .width(40.0)
                                .show_ui(ui, |ui| {
                                    for n in 1..=16 {
                                        ui.selectable_value(
                                            &mut self.draft.tab_size,
                                            n,
                                            n.to_string(),
                                        );
                                    }
                                });
                            ui.end_row();
                        });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // ── Format-Specific ──
                    ui.label(egui::RichText::new("Format-Specific").strong().size(13.0));
                    ui.add_space(4.0);
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

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // ── Files ──
                    ui.label(egui::RichText::new("Files").strong().size(13.0));
                    ui.add_space(4.0);
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
