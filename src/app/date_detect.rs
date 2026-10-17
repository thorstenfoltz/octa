//! Columns > Detect dates: the load-time date check, run when asked, on a
//! worker so the window never waits for it.
//!
//! Only columns whose first values look like dates are copied for the worker
//! (`date_infer::column_looks_like_dates`), so a wide text table costs a
//! glance per column, not a copy of it. While the job runs the tab is
//! read-only (`is_readonly`), does not sort or stream in more rows, so the
//! copy still lines up with the table when the answer comes back. The
//! columns it turns into dates go in as one `ConvertColumn` per column,
//! folded into a single Ctrl+Z.

use eframe::egui;
use octa::data::{CellValue, UndoAction, date_infer};
use octa::i18n::t;

use crate::app::file_io::passes::{DateFindings, sort_date_outcome};
use crate::app::pushdown::{ServerTask, spawn_local_task};
use crate::app::state::OctaApp;

/// A Detect dates run in flight on one tab.
pub(crate) struct DateDetectJob {
    /// The columns sent, with the names they had, which must still be
    /// theirs when the answer arrives.
    columns: Vec<(usize, String)>,
    /// Rows the copy holds.
    rows: usize,
    task: ServerTask<Detected>,
}

/// One column the worker turned into dates.
struct Converted {
    col_idx: usize,
    data_type: &'static str,
    /// The values before, for undo; also what the copy held.
    old_values: Vec<CellValue>,
    /// The values after, for redo.
    new_values: Vec<CellValue>,
    /// The values after again, moved into the table.
    cells: Vec<CellValue>,
}

struct Detected {
    findings: DateFindings,
    converted: Vec<Converted>,
}

/// The worker: the same check the load runs, on the copied columns.
fn detect(columns: Vec<(usize, String)>, copies: Vec<Vec<CellValue>>) -> Detected {
    let mut findings = DateFindings::default();
    let mut converted = Vec::new();
    for ((col_idx, name), old_values) in columns.into_iter().zip(copies) {
        let strings: Option<Vec<Option<&str>>> = old_values
            .iter()
            .map(|c| match c {
                CellValue::String(s) => Some(Some(s.as_str())),
                CellValue::Null => Some(None),
                _ => None,
            })
            .collect();
        let Some(strings) = strings else {
            continue;
        };
        let outcome = date_infer::infer_column(&strings);
        let Some(promotion) = sort_date_outcome(&mut findings, col_idx, &name, outcome) else {
            continue;
        };
        let new_values: Vec<CellValue> = old_values
            .iter()
            .map(|c| promotion.cell(c).unwrap_or_else(|| c.clone()))
            .collect();
        findings.promoted.push(col_idx);
        findings.casts.push((col_idx, promotion));
        converted.push(Converted {
            col_idx,
            data_type: promotion.data_type(),
            cells: new_values.clone(),
            old_values,
            new_values,
        });
    }
    Detected {
        findings,
        converted,
    }
}

impl OctaApp {
    /// Start Detect dates on the active tab.
    pub(crate) fn start_date_detection(&mut self) {
        if self.is_readonly() {
            return;
        }
        let idx = self.active_tab;
        let tab = &self.tabs[idx];
        let say = |key: &str| Some((t(key), std::time::Instant::now()));
        if tab.bg_row_buffer.is_some() {
            self.status_message = say("detect_dates.wait_rows");
            return;
        }
        let table = &tab.table;
        let columns: Vec<(usize, String)> = (0..table.col_count())
            .filter(|&c| date_infer::column_looks_like_dates(table, c))
            .map(|c| (c, table.columns[c].name.clone()))
            .collect();
        if columns.is_empty() {
            self.status_message = say("detect_dates.none");
            return;
        }
        // What the user sees, pending edits included, as Change type reads it.
        let rows = table.row_count();
        let copies: Vec<Vec<CellValue>> = columns
            .iter()
            .map(|&(c, _)| {
                (0..rows)
                    .map(|r| table.get(r, c).cloned().unwrap_or(CellValue::Null))
                    .collect()
            })
            .collect();
        // No `long_tasks` token: the status bar's busy hint names the job
        // while any tab runs one, and the finish notification asks it first.
        let sent = columns.clone();
        let task = spawn_local_task(move || detect(sent, copies));
        self.tabs[idx].date_detect = Some(DateDetectJob {
            columns,
            rows,
            task,
        });
    }

    /// Once per frame: take in every finished Detect dates.
    pub(crate) fn poll_date_detection(&mut self, ctx: &egui::Context) {
        for idx in 0..self.tabs.len() {
            let Some(job) = &self.tabs[idx].date_detect else {
                continue;
            };
            let Some(result) = job.task.poll_result() else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                continue;
            };
            let Some(job) = self.tabs[idx].date_detect.take() else {
                continue;
            };
            let message = match result {
                Ok(detected) => self.finish_date_detection(idx, job, detected),
                Err(e) => t("detect_dates.failed").replace("{error}", &e),
            };
            self.status_message = Some((message, std::time::Instant::now()));
        }
    }

    /// Put what the worker found into tab `idx`; the status line to show.
    fn finish_date_detection(&mut self, idx: usize, job: DateDetectJob, found: Detected) -> String {
        let table = &mut self.tabs[idx].table;
        let unchanged = table.row_count() == job.rows
            && job.columns.iter().all(|(c, name)| {
                table.columns.get(*c).is_some_and(|ci| {
                    &ci.name == name && matches!(ci.data_type.as_str(), "Utf8" | "LargeUtf8")
                })
            });
        if !unchanged {
            return t("detect_dates.changed");
        }
        let Detected {
            findings,
            converted,
        } = found;
        let undo_from = table.undo_stack.len();
        let mut names = Vec::new();
        for conv in converted {
            let c = conv.col_idx;
            for (row, cell) in table.rows.iter_mut().zip(conv.cells) {
                if let Some(slot) = row.get_mut(c) {
                    *slot = cell;
                }
            }
            // A pending edit shows over its cell, so it is converted too, or
            // the column would show the old text where it was edited.
            let edited: Vec<(usize, usize)> =
                table.edits.keys().filter(|k| k.1 == c).copied().collect();
            for key in edited {
                if let Some(v) = conv.new_values.get(key.0) {
                    table.edits.insert(key, v.clone());
                }
            }
            let new_type = conv.data_type.to_string();
            let old_type = std::mem::replace(&mut table.columns[c].data_type, new_type.clone());
            names.push(table.columns[c].name.clone());
            table.undo_stack.push(UndoAction::ConvertColumn {
                col_idx: c,
                old_type,
                new_type,
                old_values: conv.old_values,
                new_values: conv.new_values,
            });
        }
        if !names.is_empty() {
            table.structural_changes = true;
            table.redo_stack.clear();
            table.coalesce_undo_since(undo_from);
        }
        let asked = !findings.ambiguous.is_empty() || !findings.failures.is_empty();
        self.report_date_findings(idx, findings, true);
        if !names.is_empty() {
            t("detect_dates.done")
                .replace("{n}", &names.len().to_string())
                .replace("{columns}", &names.join(", "))
        } else if asked {
            t("detect_dates.questions")
        } else {
            t("detect_dates.none")
        }
    }
}

#[cfg(test)]
#[path = "date_detect_tests.rs"]
mod tests;
