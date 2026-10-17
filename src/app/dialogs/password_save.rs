//! File -> Save with password...: the table goes into an AES-256 zip.
//!
//! The OS file picker names the file inside the zip (`report.csv`); this
//! dialog asks for the passphrase twice and says, always, that what lands on
//! disk is `report.csv.zip` and which tools open it. Only a zip is written:
//! that is the one protection Octa can produce (see `formats::encrypt`).

use eframe::egui;
use octa::i18n::t;

use crate::app::state::OctaApp;

/// One open dialog. Holds a passphrase, so there is no `Debug`.
pub(crate) struct PasswordSaveState {
    pub(crate) tab_idx: usize,
    /// The file inside the zip, as the picker returned it.
    pub(crate) plain_path: std::path::PathBuf,
    /// Never logged, never persisted unless `remember`.
    pub(crate) passphrase: String,
    pub(crate) repeat: String,
    pub(crate) remember: bool,
    pub(crate) error: Option<String>,
    /// The first field has taken focus.
    pub(crate) focused: bool,
}

impl OctaApp {
    /// Pick the file name (format included), then ask for the passphrase.
    pub(crate) fn begin_save_with_password(&mut self) {
        let dialog = self.save_picker(self.active_tab);
        if let Some(plain_path) = dialog.save_file() {
            self.password_save = Some(PasswordSaveState {
                tab_idx: self.active_tab,
                plain_path,
                passphrase: String::new(),
                repeat: String::new(),
                remember: false,
                error: None,
                focused: false,
            });
        }
    }

    /// Write the zip. `Err` carries the message to show in the dialog.
    fn write_password_zip(&mut self, st: &PasswordSaveState) -> Result<std::path::PathBuf, String> {
        let failed = |why: &str| t("password_save.failed").replace("{error}", why);
        let tab = self
            .tabs
            .get(st.tab_idx)
            .ok_or_else(|| t("password_save.tab_closed"))?;
        let zip = octa::formats::encrypt::zip_path_for(&st.plain_path);
        let entry = st
            .plain_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "data.csv".to_string());
        let result = if tab.saves_text() {
            octa::formats::encrypt::write_bytes_encrypted(
                tab.raw_content.as_deref().unwrap_or_default().as_bytes(),
                &zip,
                &entry,
                &st.passphrase,
            )
        } else {
            let mut table = tab.table.clone();
            table.apply_edits();
            let mut opts = self.settings.write_options.clone();
            if let Some(d) = crate::app::file_io::save::delimited_target(&st.plain_path) {
                let (csv, delimiter) = tab.delimited_options(&opts.csv, None, d);
                opts.csv = csv;
                opts.csv.delimiter = delimiter;
            }
            octa::formats::encrypt::write_table_encrypted(
                &table,
                &zip,
                &entry,
                &st.passphrase,
                &opts,
            )
        };
        result.map_err(|e| failed(&format!("{e:#}")))?;
        Ok(zip)
    }
}

pub(crate) fn render_password_save_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.password_save.take() else {
        return;
    };
    let zip = octa::formats::encrypt::zip_path_for(&st.plain_path);
    let name_of = |p: &std::path::Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let zip_name = name_of(&zip);
    let file_name = name_of(&st.plain_path);
    let can_save = !st.passphrase.is_empty() && st.passphrase == st.repeat;
    let (mut save, mut cancel) = (false, false);
    // Centred on first show, not anchored, so it can still be dragged away.
    let default_pos = octa::ui::dialog_chrome::center_on_first_show(ctx, egui::vec2(460.0, 280.0));
    egui::Window::new(t("password_save.title"))
        .collapsible(false)
        .resizable(false)
        .default_pos(default_pos)
        .show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(
                t("password_save.note")
                    .replace("{zip}", &zip_name)
                    .replace("{file}", &file_name),
            );
            if zip.exists() {
                ui.label(
                    egui::RichText::new(t("password_save.exists").replace("{zip}", &zip_name))
                        .color(ui.visuals().warn_fg_color),
                );
            }
            ui.add_space(6.0);
            octa::ui::control_row::control_grid(ui, "password_save_grid", |ui| {
                ui.label(t("password_save.passphrase"));
                let first = ui.add(
                    egui::TextEdit::singleline(&mut st.passphrase)
                        .password(true)
                        .desired_width(260.0),
                );
                if !st.focused {
                    first.request_focus();
                    st.focused = true;
                }
                first.on_hover_text(t("password_save.passphrase_hint"));
                ui.end_row();
                ui.label(t("password_save.repeat"));
                let again = ui.add(
                    egui::TextEdit::singleline(&mut st.repeat)
                        .password(true)
                        .desired_width(260.0),
                );
                if again.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && can_save {
                    save = true;
                }
                again.on_hover_text(t("password_save.repeat_hint"));
                ui.end_row();
            });
            if !st.repeat.is_empty() && st.passphrase != st.repeat {
                ui.label(
                    egui::RichText::new(t("password_save.mismatch"))
                        .color(ui.visuals().error_fg_color),
                );
            }
            ui.checkbox(&mut st.remember, t("password_save.remember"))
                .on_hover_text(t("password_save.remember_hint"));
            if let Some(err) = &st.error {
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, err);
            }
            octa::ui::control_row::control_line(ui, |ui| {
                let btn = ui.add_enabled(can_save, egui::Button::new(t("password_save.save")));
                if btn
                    .on_hover_text(t("password_save.save_hint"))
                    .on_disabled_hover_text(t("password_save.save_disabled_hint"))
                    .clicked()
                {
                    save = true;
                }
                if ui
                    .button(t("password_save.cancel"))
                    .on_hover_text(t("password_save.cancel_hint"))
                    .clicked()
                {
                    cancel = true;
                }
            });
        });
    if cancel {
        return;
    }
    if save && can_save {
        match app.write_password_zip(&st) {
            Ok(zip) => {
                app.status_message = Some((
                    t("password_save.saved").replace("{zip}", &zip.display().to_string()),
                    std::time::Instant::now(),
                ));
                // After the Saved status, so a keyring failure message wins.
                if st.remember {
                    app.remember_passphrase(&zip, &st.passphrase);
                }
                return;
            }
            Err(msg) => st.error = Some(msg),
        }
    }
    app.password_save = Some(st);
}
