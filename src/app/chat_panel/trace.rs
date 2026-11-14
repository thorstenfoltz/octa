//! The assistant's debug box: every request to the model and what came of it,
//! step by step. Off by default, and read-only: what it shows was recorded by
//! `chat::agent`, whether the box was open or not.

use eframe::egui;

use octa::i18n::t;
use octa::ui::status_bar::format_number;

use super::helpers::copyable_text;
use crate::app::chat::session::{TraceStep, TraceToolRun};
use crate::app::state::OctaApp;

fn seconds(millis: u128) -> String {
    format!("{:.1}", millis as f64 / 1000.0)
}

/// One line for a step: its tokens, its time and the tools it called, so a slow or
/// expensive step can be found without opening any of them.
fn step_title(number: usize, step: &TraceStep) -> String {
    let mut title = t("chat.trace_step")
        .replace("{n}", &number.to_string())
        .replace("{input}", &format_number(step.input_tokens as usize))
        .replace("{output}", &format_number(step.output_tokens as usize))
        .replace("{seconds}", &step.millis.map_or("...".to_string(), seconds));
    for run in &step.tool_runs {
        title.push_str(", ");
        title.push_str(&run.name);
    }
    title
}

/// A fold-out block of text.
fn section(ui: &mut egui::Ui, id: (usize, &str), title: String, text: &str, monospace: bool) {
    egui::CollapsingHeader::new(title)
        .id_salt(("octa_chat_trace", id))
        .show(ui, |ui| copyable_text(ui, text, monospace));
}

fn draw_tool_run(ui: &mut egui::Ui, id: (usize, usize), run: &TraceToolRun) {
    let result = if run.is_error {
        t("chat.tool_error")
    } else {
        t("chat.tool_result")
    };
    let title = format!(
        "{} {}, {} s",
        t("chat.tool_call"),
        run.name,
        seconds(run.millis)
    );
    egui::CollapsingHeader::new(title)
        .id_salt(("octa_chat_trace_tool", id))
        .show(ui, |ui| {
            copyable_text(ui, &run.input, true);
            ui.separator();
            ui.weak(result);
            copyable_text(ui, &run.output, true);
        });
}

fn draw_step(ui: &mut egui::Ui, index: usize, step: &TraceStep) {
    egui::CollapsingHeader::new(step_title(index + 1, step))
        .id_salt(("octa_chat_trace_step", index))
        .show(ui, |ui| {
            ui.weak(&step.model);
            section(
                ui,
                (index, "system"),
                t("chat.trace_system"),
                &step.system,
                false,
            );
            section(
                ui,
                (index, "tools"),
                t("chat.trace_tools").replace("{n}", &step.tools.len().to_string()),
                &step.tools.join(", "),
                false,
            );
            section(ui, (index, "sent"), t("chat.trace_sent"), &step.sent, true);
            if !step.reply.is_empty() {
                section(
                    ui,
                    (index, "reply"),
                    t("chat.trace_reply"),
                    &step.reply,
                    false,
                );
            }
            for (i, run) in step.tool_runs.iter().enumerate() {
                draw_tool_run(ui, (index, i), run);
            }
            if let Some(error) = &step.error {
                ui.colored_label(ui.visuals().error_fg_color, t("chat.tool_error"));
                copyable_text(ui, error, true);
            }
        });
}

impl OctaApp {
    /// The box itself, under the conversation. Holds the session lock while it
    /// draws rather than copying up to 200 steps of tool output every frame; only
    /// a step someone opened lays any of that text out.
    pub(super) fn render_chat_trace(&mut self, ui: &mut egui::Ui) {
        let session = self.chat.session.clone();
        let state = session.lock().unwrap();
        if state.trace.is_empty() {
            ui.weak(t("chat.trace_empty"));
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("octa_chat_trace_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for (index, step) in state.trace.iter().enumerate() {
                    draw_step(ui, index, step);
                }
            });
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
