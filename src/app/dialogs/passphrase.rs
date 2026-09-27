//! The passphrase prompt for protected files.
//!
//! Raised from the file-open path when a zip or workbook turns out to be
//! password-protected. Detection happens first, so this never appears for a
//! file that needs nothing.
//!
//! The passphrase is held in memory only for as long as the dialog is open,
//! unless the user ticks **Remember**, which writes it to the OS keyring
//! keyed by the file's path. It is never written to the settings file, never
//! logged, and the diagnostics redactor masks it a second time in case it
//! reaches a log by some other route.

use eframe::egui;

use octa::i18n::t;

use crate::app::state::OctaApp;

/// Decrypt the first readable data entry out of an encrypted zip.
///
/// An encrypted archive usually holds the one file someone zipped to
/// protect it. Taking the first readable entry opens that case without
/// making the user pick from a listing they cannot see until it is
/// decrypted, which is the chicken-and-egg the prompt exists to break.
///
/// ponytail: first entry only. An encrypted archive of several tables would
/// want the archive listing to take the secret and stay open.
fn decrypt_first_zip_entry(
    path: &std::path::Path,
    secret: &octa::formats::decrypt::OpenSecret,
) -> Result<(String, Vec<u8>), String> {
    let registry = octa::formats::FormatRegistry::new();
    let entries =
        octa::formats::archive_reader::list_zip_entries(path).map_err(|e| format!("{e:#}"))?;
    let readable = entries
        .iter()
        .filter(|e| !e.is_dir)
        .find(|e| {
            std::path::Path::new(&e.path)
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| registry.all_extensions().contains(&x.to_lowercase()))
        })
        .ok_or_else(|| t("passphrase.no_readable_entry"))?;
    let bytes = octa::formats::decrypt::open_zip_entry(path, &readable.path, secret)
        .map_err(|e| format!("{e:#}"))?;
    Ok((readable.path.clone(), bytes))
}

/// Keyring id for a protected file's passphrase.
///
/// Namespaced so it cannot collide with a chat provider key, and keyed by
/// the full path: two files with the same name in different folders are
/// different secrets.
pub(crate) fn keyring_id(path: &std::path::Path) -> String {
    format!("octa-file-passphrase:{}", path.display())
}

/// State for one prompt.
pub(crate) struct PassphraseState {
    /// The file being opened.
    pub(crate) path: std::path::PathBuf,
    /// What the user has typed. Never logged, never persisted unless
    /// `remember` is ticked.
    pub(crate) entry: String,
    /// Write the passphrase to the OS keyring on success.
    pub(crate) remember: bool,
    /// Message from the previous attempt, shown selectable so it can be
    /// copied. `None` on the first prompt.
    pub(crate) error: Option<String>,
}

impl PassphraseState {
    pub(crate) fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            entry: String::new(),
            remember: false,
            error: None,
        }
    }
}

pub(crate) fn render_passphrase_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.passphrase_prompt.take() else {
        return;
    };
    let mut open = false;
    let mut cancel = false;
    let file_name = st
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| st.path.display().to_string());

    // Centred on first show, not anchored: `Area::anchor` calls
    // `movable(false)`, so an anchored window cannot be dragged out of the
    // way. `dialog_movability_tests` enforces this across every dialog.
    let default_pos = octa::ui::dialog_chrome::center_on_first_show(ctx, egui::vec2(360.0, 200.0));
    egui::Window::new(t("passphrase.title"))
        .collapsible(false)
        .resizable(false)
        .default_pos(default_pos)
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            ui.label(t("passphrase.prompt").replace("{file}", &file_name));
            ui.add_space(6.0);

            let field = ui.add(
                egui::TextEdit::singleline(&mut st.entry)
                    .password(true)
                    .desired_width(300.0)
                    .hint_text(t("passphrase.hint")),
            );
            field.request_focus();
            // Enter opens, which is what a password field is for.
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                open = true;
            }

            ui.add_space(4.0);
            ui.checkbox(&mut st.remember, t("passphrase.remember"))
                .on_hover_text(t("passphrase.remember_hint"));

            if let Some(err) = &st.error {
                ui.add_space(6.0);
                // Selectable: an error you cannot copy is an error you
                // cannot report.
                let colour = ui.visuals().error_fg_color;
                octa::ui::message::selectable_message(ui, colour, err);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_open = !st.entry.is_empty();
                let btn = ui.add_enabled(can_open, egui::Button::new(t("passphrase.open")));
                let btn = if can_open {
                    btn.on_hover_text(t("passphrase.open_hint"))
                } else {
                    btn.on_disabled_hover_text(t("passphrase.need_entry"))
                };
                if btn.clicked() {
                    open = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(t("passphrase.cancel"))
                        .on_hover_text(t("passphrase.cancel_hint"))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        });

    if cancel {
        return;
    }
    if open && !st.entry.is_empty() {
        let path = st.path.clone();
        let secret = octa::formats::decrypt::OpenSecret::Passphrase(st.entry.clone());
        match app.open_protected(&path, &secret) {
            Ok(()) => {
                if st.remember {
                    app.remember_passphrase(&path, &st.entry);
                }
                return;
            }
            Err(reason) => {
                st.error = Some(reason);
                // Clear the field so the next attempt starts fresh rather
                // than making the user select and delete a wrong guess.
                st.entry.clear();
            }
        }
    }
    app.passphrase_prompt = Some(st);
}

impl OctaApp {
    /// Store a passphrase in the OS keyring, keyed by the file's path.
    ///
    /// Failures are reported rather than swallowed: a user who ticked
    /// Remember and was silently not obeyed would find out only when the
    /// next open asked again.
    pub(crate) fn remember_passphrase(&mut self, path: &std::path::Path, passphrase: &str) {
        let id = keyring_id(path);
        if let Err(e) =
            octa::ui::settings::secrets::set_key_for(&id, passphrase, &mut self.settings)
        {
            self.status_message = Some((
                format!("{}: {e}", t("passphrase.remember_failed")),
                std::time::Instant::now(),
            ));
        }
    }

    /// A passphrase already stored for this file, if any.
    pub(crate) fn remembered_passphrase(&self, path: &std::path::Path) -> Option<String> {
        octa::ui::settings::secrets::get_key_for(&keyring_id(path), None, &self.settings)
    }
}

impl OctaApp {
    /// Open a protected file with `secret`, decrypting it into a temporary
    /// plain copy that the ordinary readers can take.
    ///
    /// Returns the (already localised) reason on failure so the prompt can
    /// show it and ask again. The temporary file holds decrypted data, so it
    /// goes in the OS temp directory with a restrictive name and is handed
    /// straight to the loader.
    pub(crate) fn open_protected(
        &mut self,
        path: &std::path::Path,
        secret: &octa::formats::decrypt::OpenSecret,
    ) -> Result<(), String> {
        // Encrypted zips only. A protected workbook is detected and
        // explained but not opened: see `decrypt::xlsx_needs_passphrase`
        // for why its decryption was dropped.
        let (entry, plain) = decrypt_first_zip_entry(path, secret)?;
        let suffix = std::path::Path::new(&entry)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_else(|| ".csv".to_string());
        let tmp = tempfile::Builder::new()
            .prefix("octa-open-")
            .suffix(&suffix)
            .tempfile()
            .map_err(|e| format!("{e}"))?;
        std::io::Write::write_all(&mut tmp.as_file(), &plain).map_err(|e| format!("{e}"))?;
        let tmp_path = tmp.path().to_path_buf();
        // Kept past the guard: the loader reads it after this returns, and
        // the OS reclaims it when the session ends.
        let _ = tmp.keep();
        self.passphrase_prompt = None;
        self.load_file(tmp_path);
        Ok(())
    }
}

impl OctaApp {
    /// Raise the passphrase prompt when `path` is protected, returning
    /// `true` when the ordinary load must not also run.
    ///
    /// A passphrase already in the keyring is tried silently first: being
    /// asked again for a file you told Octa to remember is the whole failure
    /// the Remember box exists to prevent.
    pub(crate) fn intercept_protected_file(&mut self, path: &std::path::Path) -> bool {
        // A protected workbook is detected so it can be named for what it
        // is, but Octa cannot open one: the decryption dependency was
        // dropped over two reachable advisories (see
        // `decrypt::xlsx_needs_passphrase`). Saying so beats a corrupt-file
        // error, and beats a prompt that could never succeed.
        if octa::formats::decrypt::xlsx_needs_passphrase(path).unwrap_or(false) {
            self.status_message = Some((
                t("passphrase.workbook_unsupported"),
                std::time::Instant::now(),
            ));
            return true;
        }
        if !octa::formats::decrypt::zip_needs_passphrase(path).unwrap_or(false) {
            return false;
        }
        if let Some(saved) = self.remembered_passphrase(path) {
            let secret = octa::formats::decrypt::OpenSecret::Passphrase(saved);
            if self.open_protected(path, &secret).is_ok() {
                return true;
            }
            // A stored passphrase that no longer works (the file was
            // re-protected) falls through to the prompt rather than failing
            // the open with a message about a secret the user forgot exists.
        }
        self.passphrase_prompt = Some(PassphraseState::new(path.to_path_buf()));
        true
    }
}
