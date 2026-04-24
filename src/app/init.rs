//! Application startup: icon rendering, recent-files persistence,
//! and [`OctaApp::new`].

use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::ui::settings::{AppSettings, SettingsDialog};

use super::state::{OctaApp, TabState, UpdateState};

/// Render an SVG source string into a window-icon bitmap.
pub(crate) fn render_icon(svg_source: &str) -> egui::IconData {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg_source, &opt).expect("Failed to parse SVG");
    let icon_size = 256u32;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(icon_size, icon_size).expect("Failed to create pixmap");
    let size = tree.size();
    let sx = icon_size as f32 / size.width();
    let sy = icon_size as f32 / size.height();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );
    egui::IconData {
        rgba: pixmap.data().to_vec(),
        width: icon_size,
        height: icon_size,
    }
}

pub(crate) const COLUMN_TYPES: &[&str] = &[
    "String",
    "Int64",
    "Float64",
    "Boolean",
    "Date32",
    "Timestamp(Microsecond, None)",
];

impl OctaApp {
    pub(crate) fn new(initial_file: Option<std::path::PathBuf>, settings: AppSettings) -> Self {
        let theme_mode = settings.default_theme;
        let search_mode = settings.default_search_mode;
        let recent_files = Self::load_recent_files();
        Self {
            tabs: vec![TabState::new(search_mode)],
            active_tab: 0,
            pending_close_tab: None,
            registry: octa::formats::FormatRegistry::new(),
            theme_mode,
            settings,
            settings_dialog: SettingsDialog::default(),
            search_focus_requested: false,
            show_close_confirm: false,
            confirmed_close: false,
            os_clipboard: arboard::Clipboard::new()
                .ok()
                .map(|c| Arc::new(Mutex::new(c))),
            logo_texture: None,
            welcome_logo_texture: None,
            initial_file,
            pending_open_file: false,
            show_open_confirm: false,
            show_about_dialog: false,
            show_documentation_dialog: false,
            show_update_dialog: false,
            show_unalign_confirm: false,
            update_state: Arc::new(Mutex::new(UpdateState::Idle)),
            status_message: None,
            recent_files,
            zoom_percent: 100,
            nav_input: String::new(),
            nav_focus_requested: false,
            show_reload_confirm: false,
            pending_table_picker: None,
            directory_tree: None,
        }
    }

    pub(crate) fn recent_files_path() -> Option<std::path::PathBuf> {
        AppSettings::config_dir().map(|d| d.join("recent.toml"))
    }

    pub(crate) fn load_recent_files() -> Vec<String> {
        #[derive(serde::Deserialize)]
        struct RecentData {
            #[serde(default)]
            files: Vec<String>,
        }
        Self::recent_files_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str::<RecentData>(&s).ok())
            .map(|d| d.files)
            .unwrap_or_default()
    }

    pub(crate) fn save_recent_files(&self) {
        #[derive(serde::Serialize)]
        struct RecentData<'a> {
            files: &'a [String],
        }
        if let Some(path) = Self::recent_files_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(contents) = toml::to_string_pretty(&RecentData {
                files: &self.recent_files,
            }) {
                let _ = std::fs::write(path, contents);
            }
        }
    }

    pub(crate) fn add_recent_file(&mut self, file_path: &str) {
        let canonical = std::fs::canonicalize(file_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file_path.to_string());
        self.recent_files.retain(|p| p != &canonical);
        self.recent_files.insert(0, canonical);
        let max = self.settings.max_recent_files;
        self.recent_files.truncate(max);
        self.save_recent_files();
    }
}
