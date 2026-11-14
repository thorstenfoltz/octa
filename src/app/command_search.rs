//! Find a command (Ctrl+K): a window searching every command. Picking one
//! queues it (`shortcuts::queue_command`) and it runs next frame as its shortcut
//! would, except that a focused text field does not hold it back
//! (`shortcuts::command_firing`): the user asked for it by name.

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use strum::IntoEnumIterator;

use octa::i18n::{t, t_english};
use octa::ui::command_match::rank;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};
use octa::ui::shortcuts::{ShortcutAction as SA, queue_command};

use super::state::OctaApp;

fn window_id() -> egui::Id {
    egui::Id::new("octa_command_search")
}

fn size_key() -> egui::Id {
    window_id().with("octa_dlg_size")
}

/// The search field. Its focus decides who gets the arrow keys, Enter and Escape.
fn field_id() -> egui::Id {
    window_id().with("query")
}

#[derive(Default)]
pub(crate) struct CommandSearchState {
    query: String,
    selected: usize,
    prev_focus: Option<egui::Id>,
    focus_set: bool,
}

/// Features that live inside a dialog and have no command of their own: the
/// name the dialog gives them, and the command that opens that dialog. Find a
/// command lists each under its own name. A new kind, method or option inside
/// a dialog gets a line here (CLAUDE.md, "Findable by name").
pub(crate) const INSIDE: &[(&str, SA)] = &[
    ("chart.seasonality_to_table", SA::OpenChart),
    // ponytail: chart kinds are not translated (`ChartKind::label`), so this
    // one is its own name; give it a key when the kinds get catalogue names.
    ("Pareto", SA::OpenChart),
    ("dialog.tc_kind_working_days", SA::OpenTimeCalc),
    ("dialog.vf_count_words", SA::ColumnValueFrequency),
    ("dialog.vf_count_pairs", SA::ColumnValueFrequency),
    ("outliers.method_neighbours", SA::OpenOutliers),
    ("calc.kind_previous", SA::OpenCalculatedColumn),
    ("calc.kind_next", SA::OpenCalculatedColumn),
    ("calc.kind_session", SA::OpenCalculatedColumn),
    ("calc.kind_abc", SA::OpenCalculatedColumn),
    ("refdata.mode_currency", SA::OpenReferenceData),
    ("refdata.mode_inflation", SA::OpenReferenceData),
    ("online.inflation", SA::OpenReferenceData),
    ("refdata.mode_places", SA::OpenReferenceData),
    ("refdata.key_postcode", SA::OpenReferenceData),
    ("refdata.key_kreis", SA::OpenReferenceData),
    ("refdata.key_nuts", SA::OpenReferenceData),
];

/// One listed command, in rank order.
struct Row {
    action: SA,
    /// The catalogue key the hover text hangs off (`<key>_hint`).
    hint_of: &'static str,
    name: String,
    place: String,
    keys: String,
    why: Option<String>,
}

impl OctaApp {
    pub(crate) fn open_command_search(&mut self, ctx: &egui::Context) {
        // Already open, perhaps minimised or with focus elsewhere: bring it back.
        if let Some(st) = &mut self.command_search {
            st.focus_set = false;
            if ctx.data(|d| d.get_temp::<DialogSize>(size_key())) == Some(DialogSize::Minimized) {
                ctx.data_mut(|d| d.insert_temp(size_key(), DialogSize::Normal));
            }
            return;
        }
        self.command_search = Some(CommandSearchState {
            prev_focus: ctx.memory(|m| m.focused()),
            ..Default::default()
        });
    }

    /// Why `a` cannot run now, in the menus' own words. The dispatch
    /// conditions in `shortcuts_dispatch.rs` stay authoritative; this only
    /// greys the row.
    pub(crate) fn command_unavailable(&self, a: SA) -> Option<String> {
        let tab = self.tabs.get(self.active_tab);
        let has_table = tab.is_some_and(|t| t.table.col_count() > 0);
        let need_table = || (!has_table).then(|| t("greyed.need_table"));
        let need_edit = || {
            need_table().or_else(|| {
                self.is_readonly()
                    .then(|| t("context_menu.header_row_readonly"))
            })
        };
        let need_db =
            || (!tab.is_some_and(|t| t.db_origin.is_some())).then(|| t("greyed.need_db_tab"));
        let need_file =
            || (!tab.is_some_and(|t| t.table.source_path.is_some())).then(|| t("greyed.need_file"));
        match a {
            SA::DeleteRow
            | SA::DuplicateRow
            | SA::InsertRowBelow
            | SA::EditCell
            | SA::LowercaseSelection
            | SA::UppercaseSelection
            | SA::Cut
            | SA::Paste
            | SA::OpenTransform
            | SA::OpenConditionalColumn
            | SA::OpenHashColumns
            | SA::OpenCalculatedColumn
            | SA::OpenClassify
            | SA::OpenAnonymize
            | SA::OpenDedupe
            | SA::OpenImpute
            | SA::OpenRetypeColumn
            | SA::OpenRenameColumns
            | SA::OpenTidyUp
            | SA::OpenTimeCalc
            | SA::FillSeries
            | SA::ApplyRecipe => need_edit(),
            SA::OpenPivot
            | SA::OpenColumnFilter
            | SA::OpenConditionalFormat
            | SA::OpenValidation
            | SA::OpenMultiSort
            | SA::OpenNumberFormat
            | SA::OpenCleanupPanel
            | SA::OpenTimeseries
            | SA::OpenCorrelation
            | SA::OpenLookups
            | SA::OpenDistCompare
            | SA::OpenReferential
            | SA::OpenTranspose
            | SA::OpenRowCompare
            | SA::OpenRandomSample
            | SA::OpenChart
            | SA::OpenSummary
            | SA::OpenQualityReport
            | SA::ColumnValueFrequency
            | SA::FindDuplicates
            | SA::OpenFuzzyDuplicates
            | SA::OpenOutliers
            | SA::OpenPii
            | SA::ExportPdf
            | SA::SaveFileAs
            | SA::SaveTableToDb
            | SA::ExportSchema
            | SA::CopyAsMarkdown
            | SA::CopyAsInList
            | SA::CopyAsJson
            | SA::CopyAsInsert
            | SA::Copy
            | SA::Mark
            | SA::FilterToMarked
            | SA::FitAllColumns
            | SA::FitAllRows
            | SA::SelectAllRows
            | SA::GoToCell
            | SA::OpenPartition
            | SA::OpenTestData => need_table(),
            SA::SaveDbSql | SA::RunSqlOnServer => need_db(),
            SA::ToggleFollow
            | SA::OpenFileVersions
            | SA::OpenTableFolder
            | SA::OpenGitCompare
            | SA::OpenFileInternals => need_file(),
            _ => None,
        }
    }

    /// The commands matching the query, best first: names starting with it,
    /// then the rest, each in the order the actions are declared.
    fn command_rows(&self, query: &str) -> Vec<Row> {
        let shortcuts = &self.settings.shortcuts;
        // (action, key of the name, name, where it lives, English name)
        let commands = SA::iter().filter(|a| a.in_command_search()).map(|a| {
            let place = t(a.place_key());
            (
                a,
                a.name_key(),
                a.display_name(),
                place,
                a.label().to_string(),
            )
        });
        let inside = INSIDE
            .iter()
            .map(|&(key, a)| (a, key, t(key), a.display_name(), t_english(key)));
        let mut rows: Vec<(u8, usize, Row)> = commands
            .chain(inside)
            .enumerate()
            .filter_map(|(i, (a, hint_of, name, place, english))| {
                let r = rank(query, &name, &place, &english)?;
                let combo = shortcuts.combo(a);
                let keys = if combo.key.is_some() {
                    combo.label()
                } else {
                    String::new()
                };
                let why = self.command_unavailable(a);
                Some((
                    r,
                    i,
                    Row {
                        action: a,
                        hint_of,
                        name,
                        place,
                        keys,
                        why,
                    },
                ))
            })
            .collect();
        rows.sort_by_key(|r| (r.0, r.1));
        rows.into_iter().map(|(_, _, row)| row).collect()
    }

    pub(crate) fn render_command_search(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.command_search.take() else {
            return;
        };
        let rows = self.command_rows(&st.query);
        let last = rows.len().saturating_sub(1);
        st.selected = st.selected.min(last);

        // Read before the box draws, so its text field never sees them, and only
        // while that field has focus: the box can stay open beside other work,
        // and these are the table's keys too.
        let (down, up, enter, escape) = if ctx.memory(|m| m.has_focus(field_id())) {
            ctx.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            })
        } else {
            Default::default()
        };
        if down {
            st.selected = (st.selected + 1).min(last);
        }
        if up {
            st.selected = st.selected.saturating_sub(1);
        }

        let mut run: Option<SA> = None;
        let mut close = escape;
        let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key()).unwrap_or_default());
        let minimized = size == DialogSize::Minimized;
        let center = center_on_first_show(ctx, egui::vec2(560.0, 440.0));
        let window = egui::Window::new("octa_command_search")
            .id(window_id())
            .title_bar(false)
            .collapsible(false)
            // Over every other dialog, so a command can be found whatever is open.
            .order(egui::Order::Foreground);
        let window = size_dialog_window(ctx, window_id(), size, window, |w| {
            w.resizable(true)
                .default_width(560.0)
                .default_height(440.0)
                .min_width(320.0)
                .min_height(200.0)
                .default_pos(center)
        });
        let inner = window.show(ctx, |ui| {
            if dialog_header(
                ui,
                "octa_command_search_header",
                egui::RichText::new(t("command_search.title")),
                &mut size,
            ) {
                close = true;
            }
            if minimized {
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| {
                let edit = ui
                    .add(
                        egui::TextEdit::singleline(&mut st.query)
                            .id(field_id())
                            // Claim Escape, or egui drops the focus at the start of
                            // the frame and the check above never sees the key.
                            .event_filter(egui::EventFilter {
                                horizontal_arrows: true,
                                vertical_arrows: true,
                                escape: true,
                                ..Default::default()
                            })
                            .hint_text(t("command_search.placeholder"))
                            .desired_width(f32::INFINITY),
                    )
                    .on_hover_text(t("command_search.box_hint"));
                if !st.focus_set {
                    edit.request_focus();
                    st.focus_set = true;
                }
                if edit.changed() {
                    st.selected = 0;
                }
                let font = egui::TextStyle::Button.resolve(ui.style());
                let strong = ui.visuals().strong_text_color();
                let weak = ui.visuals().weak_text_color();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Justified, so a row is clickable across its whole width
                        // and not only on its words.
                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            if rows.is_empty() {
                                ui.weak(t("command_search.none"));
                            }
                            for (idx, row) in rows.iter().enumerate() {
                                let mut job = LayoutJob::default();
                                let fmt = |color| TextFormat::simple(font.clone(), color);
                                job.append(&row.name, 0.0, fmt(strong));
                                job.append(&row.place, 24.0, fmt(weak));
                                if !row.keys.is_empty() {
                                    job.append(&row.keys, 24.0, fmt(weak));
                                }
                                let resp = ui.add_enabled(
                                    row.why.is_none(),
                                    egui::Button::selectable(idx == st.selected, job),
                                );
                                if idx == st.selected && (up || down) {
                                    resp.scroll_to_me(None);
                                }
                                let hint_key = format!("{}_hint", row.hint_of);
                                let hint = t(&hint_key);
                                let resp = match &row.why {
                                    Some(w) => resp.on_disabled_hover_text(w),
                                    None if hint != hint_key => resp.on_hover_text(hint),
                                    None => resp,
                                };
                                if resp.clicked() {
                                    run = Some(row.action);
                                }
                            }
                        });
                    });
            });
        });
        if let Some(inner) = inner {
            remember_dialog_rect(ctx, window_id(), size, inner.response.rect);
        }
        if enter && let Some(row) = rows.get(st.selected).filter(|r| r.why.is_none()) {
            run = Some(row.action);
        }
        if let Some(a) = run {
            queue_command(a);
            close = true;
            ctx.request_repaint();
        }
        ctx.data_mut(|d| d.insert_temp(size_key(), if close { DialogSize::Normal } else { size }));
        if close {
            if let Some(id) = st.prev_focus {
                ctx.memory_mut(|m| m.request_focus(id));
            } else {
                ctx.memory_mut(|m| m.stop_text_input());
            }
            return;
        }
        self.command_search = Some(st);
    }
}

#[cfg(test)]
#[path = "command_search_tests.rs"]
mod tests;
