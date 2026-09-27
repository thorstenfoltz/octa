//! Join-tables dialog (Analyse -> Join tables...).
//!
//! The user picks a **left** tab and a **right** tab, then one or more join
//! conditions. Each condition pairs any column of the left table with any
//! column of the right table via a comparison operator (`=`, `<`, `<=`, `>`,
//! `>=`). Column names and types need not match - both sides are cast to a
//! common type before comparing (numeric when both are numeric, else text).
//! Multiple conditions are ANDed. Semi and anti keep only left rows (with or
//! without a partner); as-of needs exactly one inequality, which picks the
//! nearest right row.
//!
//! Applying calls [`octa::data::join::join_two`] and opens the result in a new
//! tab (same pattern as the Union dialog).
//!
//! The **Spatial** type swaps the conditions for the spatial options and
//! joins by location instead ([`octa::data::spatial_join`]): the right tab is
//! the first layer, and **More layers** adds further tabs.

use eframe::egui;
use egui::RichText;

use octa::data::join::{JoinCond, JoinOp, JoinType, join_two};
use octa::data::spatial_join::{PointCols, point_cols};
use octa::ui::control_row::{control_grid, control_row, control_text_edit};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{JoinCondDraft, JoinState, OctaApp, SpatialDraft, TabState};

impl OctaApp {
    /// Build the default join state: left = active tab, right = the first other
    /// tab, one `=` condition on the first column of each.
    pub(crate) fn default_join_state(&self) -> JoinState {
        let active = self.active_tab;
        let right = (0..self.tabs.len())
            .find(|&i| i != active)
            .unwrap_or(active);
        JoinState {
            left_tab: active,
            right_tab: right,
            conds: vec![JoinCondDraft {
                left_col: 0,
                op: JoinOp::Eq,
                right_col: 0,
            }],
            join_type: JoinType::Left,
            spatial: None,
            error: None,
            size: DialogSize::default(),
        }
    }
}

fn tab_label(tab: &TabState, idx: usize) -> String {
    tab.table
        .source_path
        .as_ref()
        .and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .or_else(|| tab.custom_tab_label.clone())
        .unwrap_or_else(|| format!("Untitled {}", idx + 1))
}

fn op_label(op: JoinOp) -> &'static str {
    match op {
        JoinOp::Eq => "=",
        JoinOp::Lt => "<",
        JoinOp::Le => "<=",
        JoinOp::Gt => ">",
        JoinOp::Ge => ">=",
    }
}

fn type_label(t: JoinType) -> String {
    octa::i18n::t(match t {
        JoinType::Inner => "join.type_inner",
        JoinType::Left => "join.type_left",
        JoinType::Right => "join.type_right",
        JoinType::Full => "join.type_full",
        JoinType::Semi => "join.type_semi",
        JoinType::Anti => "join.type_anti",
        JoinType::AsOf => "join.type_asof",
    })
}

/// What one join type keeps, shown when hovering it in the list.
fn type_hint(t: JoinType) -> String {
    octa::i18n::t(match t {
        JoinType::Inner => "join.type_inner_hint",
        JoinType::Left => "join.type_left_hint",
        JoinType::Right => "join.type_right_hint",
        JoinType::Full => "join.type_full_hint",
        JoinType::Semi => "join.type_semi_hint",
        JoinType::Anti => "join.type_anti_hint",
        JoinType::AsOf => "join.type_asof_hint",
    })
}

fn col_name(app: &OctaApp, tab: usize, col: usize) -> String {
    app.tabs
        .get(tab)
        .and_then(|t| t.table.columns.get(col))
        .map(|c| c.name.clone())
        .unwrap_or_default()
}

pub(crate) fn render_join_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.join_dialog.is_none() {
        return;
    }
    if app.tabs.len() < 2 {
        app.join_dialog = None;
        return;
    }

    let mut close = false;
    let mut apply = false;
    let mut st = app.join_dialog.take().unwrap();

    // Clamp tab indices and condition column indices against the current tabs
    // (the user may have closed a tab while the dialog was open).
    let n_tabs = app.tabs.len();
    if st.left_tab >= n_tabs {
        st.left_tab = 0;
    }
    if st.right_tab >= n_tabs {
        st.right_tab = (0..n_tabs).find(|&i| i != st.left_tab).unwrap_or(0);
    }
    let left_cols: Vec<String> = app.tabs[st.left_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let right_cols: Vec<String> = app.tabs[st.right_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    for cond in &mut st.conds {
        if cond.left_col >= left_cols.len() {
            cond.left_col = 0;
        }
        if cond.right_col >= right_cols.len() {
            cond.right_col = 0;
        }
    }

    let tab_labels: Vec<String> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| tab_label(t, i))
        .collect();

    // Which tabs have points, for moving Left to one when Spatial is picked.
    let has_points: Vec<bool> = app
        .tabs
        .iter()
        .map(|t| point_cols(&t.table).is_some())
        .collect();
    // The left tab's points, for the Spatial type: a "lat, lon" or geometry
    // column description, `None` when it has none.
    let left_points: Option<String> = point_cols(&app.tabs[st.left_tab].table).map(|p| match p {
        PointCols::LatLon { lat, lon } => format!("{}, {}", left_cols[lat], left_cols[lon]),
        PointCols::Geometry(c) => left_cols[c].clone(),
    });
    if let Some(sp) = &mut st.spatial {
        sp.layers
            .retain(|&i| i < n_tabs && i != st.left_tab && i != st.right_tab);
    }

    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;
    let mut remove_cond: Option<usize> = None;
    let mut add_cond = false;
    let more_than_one = st.conds.len() > 1;

    let dialog_id = egui::Id::new("octa_join_dialog");
    let window = egui::Window::new("octa_join")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(540.0)
            .default_height(440.0)
            .min_width(420.0)
            .min_height(300.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("join_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("join.title"))
                            .strong()
                            .size(16.0),
                    );
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

        egui::Panel::bottom("join_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let no_points = st.spatial.is_some() && left_points.is_none();
                    if ui
                        .add_enabled(!no_points, egui::Button::new(octa::i18n::t("join.apply")))
                        .on_disabled_hover_text(octa::i18n::t("join.spatial_no_points"))
                        .clicked()
                    {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("join.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            // --- Left / right tab pickers and join type, one grid so the
            // three pickers start at the same x ---
            control_grid(ui, "join_pickers", |ui| {
                for (key, salt, sel) in [
                    ("join.left_label", "join_left_tab", &mut st.left_tab),
                    ("join.right_label", "join_right_tab", &mut st.right_tab),
                ] {
                    ui.label(RichText::new(octa::i18n::t(key)).strong());
                    egui::ComboBox::from_id_salt(salt)
                        .selected_text(tab_labels.get(*sel).cloned().unwrap_or_default())
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for (i, label) in tab_labels.iter().enumerate() {
                                ui.selectable_value(sel, i, label);
                            }
                        });
                    ui.end_row();
                }
                ui.label(RichText::new(octa::i18n::t("join.type_label")).strong())
                    .on_hover_text(octa::i18n::t("join.type_hint"));
                let spatial_on = st.spatial.is_some();
                let (selected, selected_hint) = if spatial_on {
                    (
                        octa::i18n::t("join.type_spatial"),
                        octa::i18n::t("join.type_spatial_hint"),
                    )
                } else {
                    (type_label(st.join_type), type_hint(st.join_type))
                };
                egui::ComboBox::from_id_salt("join_type_combo")
                    .selected_text(selected)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for t in JoinType::ALL {
                            let picked = !spatial_on && st.join_type == t;
                            if ui
                                .selectable_label(picked, type_label(t))
                                .on_hover_text(type_hint(t))
                                .clicked()
                            {
                                st.join_type = t;
                                st.spatial = None;
                            }
                        }
                        if ui
                            .selectable_label(spatial_on, octa::i18n::t("join.type_spatial"))
                            .on_hover_text(octa::i18n::t("join.type_spatial_hint"))
                            .clicked()
                            && !spatial_on
                        {
                            // The points go on the left. If the tab there has
                            // none (a regions file opened last is the active
                            // tab), move a tab that has them there, and make
                            // every other tab a layer candidate.
                            if !has_points[st.left_tab]
                                && let Some(p) = has_points.iter().position(|&h| h)
                            {
                                st.left_tab = p;
                            }
                            // The right tab is the first layer; keep it off
                            // the tab that moved to the left.
                            if st.right_tab == st.left_tab
                                && let Some(r) = (0..tab_labels.len()).find(|&i| i != st.left_tab)
                            {
                                st.right_tab = r;
                            }
                            st.spatial = Some(SpatialDraft {
                                nearest: false,
                                within_km_text: String::new(),
                                layers: std::collections::BTreeSet::new(),
                            });
                        }
                    })
                    .response
                    .on_hover_text(selected_hint);
                ui.end_row();
            });
            if let Some(sp) = &mut st.spatial {
                ui.add_space(8.0);
                ui.separator();
                spatial_section(
                    ui,
                    sp,
                    (st.left_tab, st.right_tab),
                    &tab_labels,
                    left_points.as_deref(),
                );
                if let Some(err) = &st.error {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(err)
                            .color(ui.visuals().error_fg_color)
                            .size(11.0),
                    );
                }
                return;
            }
            if st.join_type == JoinType::AsOf {
                ui.label(
                    RichText::new(octa::i18n::t("join.explain_asof"))
                        .weak()
                        .size(11.0),
                );
            }

            ui.add_space(8.0);
            ui.separator();

            // --- Conditions ---
            ui.label(
                RichText::new(octa::i18n::t("join.conditions_label"))
                    .strong()
                    .size(13.0),
            );
            ui.label(
                RichText::new(octa::i18n::t("join.conditions_hint"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("join_conds")
                .max_height(160.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (ci, cond) in st.conds.iter_mut().enumerate() {
                        control_row(ui, |ui| {
                            // Left column.
                            egui::ComboBox::from_id_salt(("join_lcol", ci))
                                .selected_text(
                                    left_cols.get(cond.left_col).cloned().unwrap_or_default(),
                                )
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    for (i, name) in left_cols.iter().enumerate() {
                                        ui.selectable_value(&mut cond.left_col, i, name);
                                    }
                                });
                            // Operator.
                            egui::ComboBox::from_id_salt(("join_op", ci))
                                .selected_text(op_label(cond.op))
                                .width(60.0)
                                .show_ui(ui, |ui| {
                                    for op in
                                        [JoinOp::Eq, JoinOp::Lt, JoinOp::Le, JoinOp::Gt, JoinOp::Ge]
                                    {
                                        ui.selectable_value(&mut cond.op, op, op_label(op));
                                    }
                                });
                            // Right column.
                            egui::ComboBox::from_id_salt(("join_rcol", ci))
                                .selected_text(
                                    right_cols.get(cond.right_col).cloned().unwrap_or_default(),
                                )
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    for (i, name) in right_cols.iter().enumerate() {
                                        ui.selectable_value(&mut cond.right_col, i, name);
                                    }
                                });
                            // Remove (only when more than one condition).
                            if more_than_one && ui.button("X").clicked() {
                                remove_cond = Some(ci);
                            }
                        });
                    }
                });

            if ui.button(octa::i18n::t("join.add_condition")).clicked() {
                add_cond = true;
            }

            if let Some(err) = &st.error {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(err)
                        .color(ui.visuals().error_fg_color)
                        .size(11.0),
                );
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if let Some(ci) = remove_cond
        && st.conds.len() > 1
    {
        st.conds.remove(ci);
    }
    if add_cond {
        st.conds.push(JoinCondDraft {
            left_col: 0,
            op: JoinOp::Eq,
            right_col: 0,
        });
    }

    if apply {
        let applied = match &st.spatial {
            Some(sp) => apply_spatial(app, &st, sp),
            None => apply_join(app, &st),
        };
        match applied {
            Ok(()) => return,
            Err(e) => st.error = Some(e),
        }
    }
    if !close {
        app.join_dialog = Some(st);
    }
}

/// Snapshot the two chosen tabs and run the two-table join, opening the result
/// in a new tab.
fn apply_join(app: &mut OctaApp, st: &JoinState) -> Result<(), String> {
    if st.left_tab == st.right_tab {
        return Err(octa::i18n::t("join.same_tab"));
    }
    if st.conds.is_empty() {
        return Err(octa::i18n::t("join.need_key"));
    }

    // Resolve condition column indices to names against the current schemas.
    let conds: Vec<JoinCond> = st
        .conds
        .iter()
        .map(|c| JoinCond {
            left_col: col_name(app, st.left_tab, c.left_col),
            op: c.op,
            right_col: col_name(app, st.right_tab, c.right_col),
        })
        .collect();

    let mut left = app.tabs[st.left_tab].table.clone();
    left.apply_edits();
    let mut right = app.tabs[st.right_tab].table.clone();
    right.apply_edits();

    let result = join_two(("l", &left), ("r", &right), &conds, st.join_type)
        .map_err(|e| format!("{e:#}"))?;

    let mut new_tab = TabState::new(app.settings.default_search_mode);
    new_tab.table = result;
    new_tab.table.source_path = None;
    new_tab.table.format_name = None;
    new_tab.custom_tab_label = Some(octa::i18n::t("join.title"));
    new_tab.filter_dirty = true;
    if new_tab.table.row_count() > 0 && new_tab.table.col_count() > 0 {
        new_tab.table_state.selected_cell = Some((0, 0));
    }
    app.tabs.push(new_tab);
    app.active_tab = app.tabs.len() - 1;
    Ok(())
}

/// The Spatial type's body: the detected points, Inside / Nearest, the
/// distance cut-off and one checkbox per layer tab.
fn spatial_section(
    ui: &mut egui::Ui,
    sp: &mut SpatialDraft,
    (left_tab, right_tab): (usize, usize),
    tab_labels: &[String],
    left_points: Option<&str>,
) {
    control_row(ui, |ui| {
        ui.label(RichText::new(octa::i18n::t("join.spatial_points")).strong());
        match left_points {
            Some(cols) => {
                ui.label(cols);
            }
            None => {
                ui.label(
                    RichText::new(octa::i18n::t("join.spatial_no_points"))
                        .color(ui.visuals().error_fg_color),
                );
            }
        }
    });
    control_row(ui, |ui| {
        ui.radio_value(&mut sp.nearest, false, octa::i18n::t("join.spatial_inside"))
            .on_hover_text(octa::i18n::t("join.spatial_inside_hint"));
        ui.radio_value(&mut sp.nearest, true, octa::i18n::t("join.spatial_nearest"))
            .on_hover_text(octa::i18n::t("join.spatial_nearest_hint"));
        if sp.nearest {
            ui.label(octa::i18n::t("join.spatial_within"))
                .on_hover_text(octa::i18n::t("join.spatial_within_hint"));
            control_text_edit(ui, 80.0, egui::TextEdit::singleline(&mut sp.within_km_text))
                .on_hover_text(octa::i18n::t("join.spatial_within_hint"));
        }
    });
    // The right tab is the first layer; any other tab can join as well.
    let extra: Vec<usize> = (0..tab_labels.len())
        .filter(|&i| i != left_tab && i != right_tab)
        .collect();
    if extra.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(RichText::new(octa::i18n::t("join.spatial_more_layers")).strong())
        .on_hover_text(octa::i18n::t("join.spatial_layer_hint"));
    egui::ScrollArea::vertical()
        .id_salt("join_spatial_layers")
        .max_height(180.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for &i in &extra {
                let mut on = sp.layers.contains(&i);
                if ui
                    .checkbox(&mut on, &tab_labels[i])
                    .on_hover_text(octa::i18n::t("join.spatial_layer_hint"))
                    .changed()
                {
                    if on {
                        sp.layers.insert(i);
                    } else {
                        sp.layers.remove(&i);
                    }
                }
            }
        });
}

/// Join the left tab's points against the right tab and every ticked extra
/// layer by location, opening the result in a new tab.
fn apply_spatial(app: &mut OctaApp, st: &JoinState, sp: &SpatialDraft) -> Result<(), String> {
    use octa::data::spatial_join::{Layer, SpatialOp, prefix_for, spatial_join};
    if st.left_tab == st.right_tab {
        return Err(octa::i18n::t("join.same_tab"));
    }
    let layer_tabs: Vec<usize> = std::iter::once(st.right_tab)
        .chain(sp.layers.iter().copied())
        .filter(|&i| i != st.left_tab)
        .collect();
    let mut points = app.tabs[st.left_tab].table.clone();
    points.apply_edits();
    let cols = point_cols(&points).ok_or_else(|| octa::i18n::t("join.spatial_no_points"))?;
    let tables: Vec<(String, octa::data::DataTable)> = layer_tabs
        .iter()
        .map(|&i| {
            let mut t = app.tabs[i].table.clone();
            t.apply_edits();
            (prefix_for(&tab_label(&app.tabs[i], i)), t)
        })
        .collect();
    let layers: Vec<Layer> = tables
        .iter()
        .map(|(name, table)| Layer {
            name: name.clone(),
            table,
        })
        .collect();
    let within_km = match sp.within_km_text.trim().replace(',', ".") {
        s if s.is_empty() => None,
        s => Some(
            s.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(|| octa::i18n::t("join.spatial_within_bad"))?,
        ),
    };
    let op = if sp.nearest {
        SpatialOp::Nearest { within_km }
    } else {
        SpatialOp::Inside
    };
    let r = spatial_join(&points, cols, &layers, op).map_err(|e| format!("{e:#}"))?;
    let mut notes = Vec::new();
    if r.multi_match > 0 {
        notes.push(
            octa::i18n::t("join.spatial_multi_note").replace("{count}", &r.multi_match.to_string()),
        );
    }
    if r.no_point > 0 {
        notes.push(
            octa::i18n::t("join.spatial_no_point_note").replace("{count}", &r.no_point.to_string()),
        );
    }
    let mut new_tab = TabState::new(app.settings.default_search_mode);
    new_tab.table = r.table;
    new_tab.table.source_path = None;
    new_tab.table.format_name = None;
    new_tab.custom_tab_label = Some(octa::i18n::t("join.spatial_tab_label"));
    new_tab.parse_error_banner = (!notes.is_empty()).then(|| notes.join(" "));
    new_tab.filter_dirty = true;
    app.tabs.push(new_tab);
    app.active_tab = app.tabs.len() - 1;
    Ok(())
}
