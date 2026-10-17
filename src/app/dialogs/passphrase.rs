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
        .filter(|e| !opens_outside_private_folder(std::path::Path::new(&e.path)))
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

/// Entries an encrypted zip never opens: compressed files and archives.
/// Opening one writes plain data into the shared temp folder, outside the
/// private one: decompression leaves the inner file there, and the archive
/// listing's Open selected entry extracts members there. Decided here, where
/// the entry is chosen, rather than trusting what the registry claims.
fn opens_outside_private_folder(entry: &std::path::Path) -> bool {
    use octa::formats::FormatReader;
    let name = entry
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let archive_exts = octa::formats::archive_reader::ArchiveReader.extensions();
    octa::formats::compression::detect_codec(entry).is_some()
        // `detect_codec` leaves `.tar.gz` to the archive side on purpose.
        || name.ends_with(".tar.gz")
        || std::path::Path::new(&name)
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| archive_exts.contains(&x))
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
            octa::ui::control_row::control_line(ui, |ui| {
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
    /// Open a protected file with `secret`, decrypting it into a plain copy
    /// that the ordinary readers can take.
    ///
    /// Returns the (already localised) reason on failure so the prompt can
    /// show it and ask again. The copy lives in a private folder that
    /// belongs to the tab (`ProtectedOrigin`): Save puts it back into the
    /// zip, and the folder goes when the last tab holding it does.
    pub(crate) fn open_protected(
        &mut self,
        path: &std::path::Path,
        secret: &octa::formats::decrypt::OpenSecret,
    ) -> Result<(), String> {
        // Encrypted zips only. A protected workbook is detected and
        // explained but not opened: see `decrypt::xlsx_needs_passphrase`
        // for why its decryption was dropped.
        let (entry, plain) = decrypt_first_zip_entry(path, secret)?;
        let temp = octa::formats::encrypt::ProtectedTempDir::new().map_err(|e| format!("{e:#}"))?;
        // The entry's own name, so the tab and the format registry see
        // `report.csv`, not a random temp name.
        let file_name = std::path::Path::new(&entry)
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_else(|| std::ffi::OsString::from("data.csv"));
        let temp_file = temp.path().join(file_name);
        std::fs::write(&temp_file, &plain).map_err(|e| format!("{e:#}"))?;
        let passphrase = match secret {
            octa::formats::decrypt::OpenSecret::Passphrase(p) => p.clone(),
            octa::formats::decrypt::OpenSecret::None => String::new(),
        };
        self.pending_protected_origins
            .push(crate::app::state::ProtectedOrigin {
                zip: path.to_path_buf(),
                entry,
                passphrase,
                temp_file: temp_file.clone(),
                _temp: std::sync::Arc::new(temp),
            });
        self.passphrase_prompt = None;
        self.load_file(temp_file);
        // A read that failed outright leaves nothing to claim the copy.
        self.release_unclaimed_protected_origins();
        Ok(())
    }

    /// Drop the pending origins nothing is opening any more: a failed read,
    /// a cancelled picker or question. Their plain copies go now, not at
    /// exit. Runs after every protected open and once per frame.
    ///
    /// The in-flight states are every way `load_file` defers a single file;
    /// all of them resolve synchronously into a claim or into nothing. The
    /// large-file conversion job carries no path, but a protected copy never
    /// reaches it (`intercept_large_file`).
    pub(crate) fn release_unclaimed_protected_origins(&mut self) {
        if self.pending_protected_origins.is_empty() {
            return;
        }
        let in_flight = [
            self.pending_load.as_ref().map(|p| p.path.as_path()),
            self.pending_table_picker.as_ref().map(|p| p.path.as_path()),
            self.pending_sheet_picker.as_ref().map(|p| p.path.as_path()),
            self.pending_large_file_notice
                .as_ref()
                .map(|p| p.path.as_path()),
            self.pending_file_repair.as_ref().map(|p| p.path.as_path()),
        ];
        retain_in_flight(&mut self.pending_protected_origins, &in_flight);
    }

    /// Put tab `tab_idx`, just placed on `path`, on the protected zip that
    /// path is the decrypted copy of, or on none. Every site that places a
    /// tab on a file calls this, so no tab holds a plain copy without the
    /// handle that saves it back and removes it.
    pub(crate) fn claim_protected_origin(&mut self, tab_idx: usize, path: &std::path::Path) {
        let origin = take_protected_origin(&mut self.pending_protected_origins, &self.tabs, path);
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        match &origin {
            Some(o) => {
                tab.custom_tab_label = o.zip.file_name().map(|n| n.to_string_lossy().to_string());
            }
            // A tab reused for an unrelated file must not keep the zip's name.
            None if tab.protected_origin.is_some() => tab.custom_tab_label = None,
            None => {}
        }
        tab.protected_origin = origin;
    }
}

/// Keep only the pending origins whose plain copy something is still
/// opening; dropping the others removes their folders.
fn retain_in_flight(
    pending: &mut Vec<crate::app::state::ProtectedOrigin>,
    in_flight: &[Option<&std::path::Path>],
) {
    pending.retain(|o| in_flight.contains(&Some(o.temp_file.as_path())));
}

/// Is `path` the decrypted copy of a protected zip, waiting to be claimed or
/// open in a tab?
pub(crate) fn is_protected_copy(
    pending: &[crate::app::state::ProtectedOrigin],
    tabs: &[crate::app::state::TabState],
    path: &std::path::Path,
) -> bool {
    pending
        .iter()
        .chain(tabs.iter().filter_map(|t| t.protected_origin.as_ref()))
        .any(|o| o.temp_file == path)
}

/// The protected-zip origin of the decrypted copy at `path`: the pending one
/// recorded for exactly that path, else the one a tab already on it holds
/// (Refresh, Reopen as, the other sheets of a workbook), shared so the
/// folder lives as long as any of them. Pending origins for other paths stay
/// where they are, since their own reads may still be running.
pub(crate) fn take_protected_origin(
    pending: &mut Vec<crate::app::state::ProtectedOrigin>,
    tabs: &[crate::app::state::TabState],
    path: &std::path::Path,
) -> Option<crate::app::state::ProtectedOrigin> {
    if let Some(i) = pending.iter().position(|o| o.temp_file == path) {
        return Some(pending.swap_remove(i));
    }
    tabs.iter()
        .filter_map(|t| t.protected_origin.as_ref())
        .find(|o| o.temp_file == path)
        .cloned()
}

#[cfg(test)]
#[path = "passphrase_tests.rs"]
mod tests;

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
