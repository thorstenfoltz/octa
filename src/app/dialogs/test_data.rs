//! Generate test data dialog (Data -> Generate test data...).
//!
//! Tick one or more open tabs; every column gets a generator picked by
//! [`octa::data::test_data::profile_table`], which the user can change per
//! column. Links between the ticked tabs are found by the Join key finder so
//! the generated tables still join. **Generate** opens one new tab per table.
//!
//! Profiling runs on the UI thread when the ticked set changes.
//! ponytail: synchronous; a worker when a huge tab makes the dialog stutter.

use eframe::egui;
use egui::RichText;

use octa::data::test_data::{
    Generator, TablePlan, apply_links, clock_seed, generate, profile_table, suggest_links,
};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, TabState};

pub(crate) struct TestDataState {
    pub(crate) size: DialogSize,
    /// Ticked tab indices, in tab order.
    pub(crate) tabs: Vec<usize>,
    /// The tabs `plans` were built for; a different tick set rebuilds them.
    planned_for: Vec<usize>,
    plans: Vec<TablePlan>,
    /// Empty = as many rows as each real table.
    rows: String,
    seed: String,
    error: Option<String>,
}

impl OctaApp {
    pub(crate) fn open_test_data_dialog(&mut self) {
        self.test_data_dialog = Some(TestDataState {
            size: DialogSize::Normal,
            tabs: vec![self.active_tab],
            planned_for: Vec::new(),
            plans: Vec::new(),
            rows: String::new(),
            seed: clock_seed().to_string(),
            error: None,
        });
    }
}

/// What the generator dropdown shows for `g`.
fn label(g: &Generator, plans: &[TablePlan]) -> String {
    match g {
        Generator::Link { table, column } => {
            let target = plans
                .get(*table)
                .and_then(|p| Some(format!("{}.{}", p.name, p.columns.get(*column)?.name)))
                .unwrap_or_default();
            t("testdata.gen_link").replace("{target}", &target)
        }
        Generator::Category { .. } if !g.keeps_real_values() => t("testdata.gen_category_renamed"),
        g => t(&format!("testdata.gen_{}", g.id())),
    }
}

fn replan(app: &OctaApp, st: &mut TestDataState) {
    let tables: Vec<_> = st
        .tabs
        .iter()
        .filter_map(|&i| app.tabs.get(i))
        .map(|tab| {
            let mut t = tab.table.clone();
            t.apply_edits();
            (tab.title_display(), t)
        })
        .collect();
    st.plans = tables.iter().map(|(n, t)| profile_table(t, n)).collect();
    let refs: Vec<_> = tables.iter().map(|(_, t)| t).collect();
    apply_links(&mut st.plans, &suggest_links(&refs));
    st.planned_for = st.tabs.clone();
}

pub(crate) fn render_test_data_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.test_data_dialog.take() else {
        return;
    };
    st.tabs.retain(|&i| i < app.tabs.len());
    if st.tabs != st.planned_for {
        replan(app, &mut st);
    }

    let mut close = false;
    let mut run = false;
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let tabs: Vec<(usize, String)> = (0..app.tabs.len())
        .filter(|&i| app.tabs[i].table.col_count() > 0)
        .map(|i| (i, app.tabs[i].title_display()))
        .collect();

    let dialog_id = egui::Id::new("octa_test_data_dialog");
    let window = egui::Window::new("octa_test_data")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(620.0)
            .default_height(520.0)
            .min_width(420.0)
            .min_height(240.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("testdata_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("testdata.title")).strong().size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }

        egui::Panel::bottom("testdata_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                if let Some(e) = &st.error {
                    octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !st.plans.is_empty(),
                            egui::Button::new(t("testdata.generate")),
                        )
                        .on_hover_text(t("testdata.generate_hint"))
                        .on_disabled_hover_text(t("testdata.need_source"))
                        .clicked()
                    {
                        run = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                RichText::new(t("testdata.intro"))
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(6.0);

            ui.label(RichText::new(t("testdata.sources")).strong())
                .on_hover_text(t("testdata.sources_hint"));
            ui.horizontal_wrapped(|ui| {
                for (i, name) in &tabs {
                    let mut on = st.tabs.contains(i);
                    if ui
                        .checkbox(&mut on, name)
                        .on_hover_text(t("testdata.sources_hint"))
                        .changed()
                    {
                        if on {
                            st.tabs.push(*i);
                            st.tabs.sort_unstable();
                        } else {
                            st.tabs.retain(|x| x != i);
                        }
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label(t("testdata.rows"))
                    .on_hover_text(t("testdata.rows_hint"));
                ui.add(
                    egui::TextEdit::singleline(&mut st.rows)
                        .desired_width(90.0)
                        .hint_text(t("testdata.rows_same")),
                )
                .on_hover_text(t("testdata.rows_hint"));
                ui.add_space(12.0);
                ui.label(t("testdata.seed"))
                    .on_hover_text(t("testdata.seed_hint"));
                ui.add(egui::TextEdit::singleline(&mut st.seed).desired_width(120.0))
                    .on_hover_text(t("testdata.seed_hint"));
            });
            ui.separator();

            let plans_snapshot = st.plans.clone();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (pi, plan) in st.plans.iter_mut().enumerate() {
                        ui.label(RichText::new(&plan.name).strong());
                        egui::Grid::new(("testdata_grid", pi))
                            .num_columns(4)
                            .striped(true)
                            .spacing([12.0, 4.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new(t("testdata.col_column")).weak());
                                ui.label(RichText::new(t("testdata.col_generator")).weak())
                                    .on_hover_text(t("testdata.generator_hint"));
                                ui.label(RichText::new(t("testdata.col_empty")).weak())
                                    .on_hover_text(t("testdata.empty_hint"));
                                ui.label("");
                                ui.end_row();
                                for (ci, col) in plan.columns.iter_mut().enumerate() {
                                    ui.label(&col.name);
                                    egui::ComboBox::from_id_salt(("testdata_gen", pi, ci))
                                        .selected_text(label(&col.generator, &plans_snapshot))
                                        .width(220.0)
                                        .show_ui(ui, |ui| {
                                            for alt in &col.alternatives {
                                                if ui
                                                    .selectable_label(
                                                        *alt == col.generator,
                                                        label(alt, &plans_snapshot),
                                                    )
                                                    .clicked()
                                                {
                                                    col.generator = alt.clone();
                                                }
                                            }
                                        })
                                        .response
                                        .on_hover_text(t("testdata.generator_hint"));
                                    ui.label(format!("{:.0} %", col.null_share * 100.0))
                                        .on_hover_text(t("testdata.empty_hint"));
                                    if col.generator.keeps_real_values() {
                                        ui.label(
                                            RichText::new(t("testdata.real_values"))
                                                .color(ui.visuals().warn_fg_color),
                                        )
                                        .on_hover_text(t("testdata.real_values_hint"));
                                    } else {
                                        ui.label("");
                                    }
                                    ui.end_row();
                                }
                            });
                        ui.add_space(8.0);
                    }
                });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if run {
        match run_generate(app, &st) {
            Ok(()) => return,
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.test_data_dialog = Some(st);
    }
}

/// Generate every plan and open one tab per table.
fn run_generate(app: &mut OctaApp, st: &TestDataState) -> Result<(), String> {
    let seed: u64 = st.seed.trim().parse().map_err(|_| t("testdata.bad_seed"))?;
    let mut plans = st.plans.clone();
    if !st.rows.trim().is_empty() {
        let n: usize = st.rows.trim().parse().map_err(|_| t("testdata.bad_rows"))?;
        for p in &mut plans {
            p.rows = n;
        }
    }
    let tables = generate(&plans, seed).map_err(|e| format!("{e:#}"))?;
    for (plan, table) in plans.iter().zip(tables) {
        let mut tab = TabState::new(app.settings.default_search_mode);
        tab.table = table;
        tab.custom_tab_label = Some(t("testdata.tab_label").replace("{name}", &plan.name));
        tab.filter_dirty = true;
        app.tabs.push(tab);
    }
    app.active_tab = app.tabs.len() - 1;
    app.status_message = Some((
        t("testdata.done")
            .replace("{n}", &plans.len().to_string())
            .replace("{seed}", &seed.to_string()),
        std::time::Instant::now(),
    ));
    Ok(())
}
