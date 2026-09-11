//! Modal that asks the user how to interpret an ambiguous date column. Fires
//! once per column whose values match more than one date layout (e.g.
//! `02/03/2024` is consistent with both DD/MM and MM/DD). Multiple ambiguous
//! columns queue: the head of `pending_date_pickers` is the active dialog.

use std::collections::VecDeque;

use eframe::egui;
use egui::RichText;
use octa::data::date_infer::{self, DateLayout, DateTimeLayout};

use super::super::state::{DateAmbiguity, OctaApp};
use octa::ui::settings::{
    DialogSize, center_on_first_show, draw_window_controls, remember_dialog_rect,
    size_dialog_window,
};

#[derive(Clone, Copy, PartialEq)]
enum Choice {
    Date(DateLayout),
    DateTime(DateTimeLayout),
    Skip,
}

/// Which queued columns can take the same answer.
///
/// A wide file can raise dozens of these, and answering each one separately
/// is the difference between a question and an obstruction. Only columns that
/// actually offer the chosen layout are settled; a column with a different
/// set of candidates stays queued and gets asked properly.
fn takes_same_answer(queue: &VecDeque<DateAmbiguity>, choice: Choice) -> Vec<usize> {
    queue
        .iter()
        .enumerate()
        .filter(|(_, q)| match choice {
            Choice::Date(l) => q.date_candidates.contains(&l),
            Choice::DateTime(l) => q.datetime_candidates.contains(&l),
            // "Leave as text" is an answer every column can take.
            Choice::Skip => true,
        })
        .map(|(i, _)| i)
        .collect()
}

/// Apply one answer to one queued column.
fn apply_to(app: &mut OctaApp, q: &DateAmbiguity, choice: Choice) {
    if q.tab_idx >= app.tabs.len() {
        return;
    }
    let tab = &mut app.tabs[q.tab_idx];
    match choice {
        Choice::Date(layout) => date_infer::apply_date(&mut tab.table, q.col_idx, layout),
        Choice::DateTime(layout) => date_infer::apply_datetime(&mut tab.table, q.col_idx, layout),
        Choice::Skip => {}
    }
    tab.filter_dirty = true;
    tab.table_state.invalidate_row_heights();
}

pub(crate) fn render_date_ambiguity_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(state) = app.pending_date_pickers.front() else {
        return;
    };

    let title = octa::i18n::t("dialog.date_title");
    let col_name = state.col_name.clone();
    let samples = state.samples.clone();
    let date_candidates = state.date_candidates.clone();
    let datetime_candidates = state.datetime_candidates.clone();
    let queued = app.pending_date_pickers.len();

    let mut choice: Option<Choice> = None;
    // No state struct for one checkbox; egui's temp memory is where the
    // validation dialog keeps its message too.
    let apply_all_id = egui::Id::new("octa_date_ambiguity_apply_all");
    let mut apply_all = ctx.data(|d| d.get_temp::<bool>(apply_all_id).unwrap_or(false));

    let dialog_id = egui::Id::new("octa_date_ambiguity_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;
    let mut chrome_close = false;

    let center = center_on_first_show(ctx, egui::vec2(520.0, 380.0));
    let window = egui::Window::new("octa_date_ambiguity")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(380.0)
            .min_width(360.0)
            .min_height(220.0)
            .default_pos(center)
    });
    let inner = window.show(ctx, |ui| {
        egui::Panel::top("date_ambiguity_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(title.clone()).strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            chrome_close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{}: '{}'",
                        octa::i18n::t("dialog.date_column"),
                        col_name
                    ))
                    .strong(),
                );
                // Say how many more are coming, so a file with forty date
                // columns does not feel like an endless stream of modals.
                if queued > 1 {
                    ui.label(
                        RichText::new(
                            octa::i18n::t("dialog.date_queue").replace("{n}", &queued.to_string()),
                        )
                        .weak(),
                    );
                }
            });
            ui.add_space(4.0);
            ui.label(octa::i18n::t("dialog.date_body"));
            ui.add_space(8.0);
            ui.label(RichText::new(octa::i18n::t("dialog.date_samples")).strong());
            for s in &samples {
                ui.label(RichText::new(format!("  {s}")).monospace());
            }
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            for layout in &date_candidates {
                if ui.button(layout.label()).clicked() {
                    choice = Some(Choice::Date(*layout));
                }
            }
            for layout in &datetime_candidates {
                if ui.button(layout.label()).clicked() {
                    choice = Some(Choice::DateTime(*layout));
                }
            }
            ui.add_space(8.0);
            if ui
                .button(octa::i18n::t("dialog.date_leave_as_text"))
                .clicked()
            {
                choice = Some(Choice::Skip);
            }
            if queued > 1 {
                ui.add_space(6.0);
                ui.separator();
                ui.checkbox(&mut apply_all, octa::i18n::t("dialog.date_apply_all"))
                    .on_hover_text(octa::i18n::t("dialog.date_apply_all_hint"));
            }
        });
    });
    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if chrome_close {
                DialogSize::Normal
            } else {
                size
            },
        )
    });
    if chrome_close {
        choice = Some(Choice::Skip);
    }

    ctx.data_mut(|d| d.insert_temp(apply_all_id, apply_all));

    let Some(c) = choice else {
        return;
    };
    if let Some(front) = app.pending_date_pickers.pop_front() {
        apply_to(app, &front, c);
    }
    if apply_all {
        // Highest index first, so removing one does not shift the next.
        for i in takes_same_answer(&app.pending_date_pickers, c)
            .into_iter()
            .rev()
        {
            if let Some(q) = app.pending_date_pickers.remove(i) {
                apply_to(app, &q, c);
            }
        }
        ctx.data_mut(|d| d.insert_temp(apply_all_id, false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(name: &str, dates: &[DateLayout], datetimes: &[DateTimeLayout]) -> DateAmbiguity {
        DateAmbiguity {
            tab_idx: 0,
            col_idx: 0,
            col_name: name.to_string(),
            samples: Vec::new(),
            date_candidates: dates.to_vec(),
            datetime_candidates: datetimes.to_vec(),
        }
    }

    /// "Use this for all remaining" must only settle columns that actually
    /// offer the chosen layout. Forcing it on a column with a different set
    /// of candidates would answer a question nobody asked, and silently
    /// reinterpret its values.
    #[test]
    fn apply_to_all_skips_columns_that_do_not_offer_the_layout() {
        let queue: VecDeque<DateAmbiguity> = [
            q("a", &[DateLayout::DmySlash, DateLayout::MdySlash], &[]),
            q("b", &[DateLayout::MdySlash], &[]),
            q("c", &[], &[DateTimeLayout::DmySlashSpace]),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            takes_same_answer(&queue, Choice::Date(DateLayout::DmySlash)),
            [0]
        );
        assert_eq!(
            takes_same_answer(&queue, Choice::Date(DateLayout::MdySlash)),
            [0, 1]
        );
        assert_eq!(
            takes_same_answer(&queue, Choice::DateTime(DateTimeLayout::DmySlashSpace)),
            [2]
        );
    }

    /// Leaving a column as text changes nothing, so every queued column can
    /// take that answer: it is the one that clears the whole queue.
    #[test]
    fn leaving_as_text_settles_every_remaining_column() {
        let queue: VecDeque<DateAmbiguity> = [
            q("a", &[DateLayout::DmySlash], &[]),
            q("b", &[], &[DateTimeLayout::MdySlashSpace]),
        ]
        .into_iter()
        .collect();
        assert_eq!(takes_same_answer(&queue, Choice::Skip), [0, 1]);
    }
}
