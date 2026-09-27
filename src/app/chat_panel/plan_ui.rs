//! The Approve / Discard bar under a plan.
//!
//! Plan mode gates the whole turn: the assistant gets no tools, so it can only
//! describe what it would do. This bar is the gate. Approving re-asks the same
//! conversation in Data mode, where the tools are, so the plan the user read is
//! the plan that runs.
//!
//! Interaction-struct shaped, and for a concrete reason: the transcript holds
//! the chat session lock while it draws, and approving sends a message, which
//! takes that same lock. The renderer reports; `OctaApp` acts afterwards.

use egui::RichText;
use octa::i18n::t;

use crate::app::state::OctaApp;

use super::ChatTurnMode;

/// What the user pressed this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanAction {
    pub approve: bool,
    pub discard: bool,
}

/// Draw the bar. Returns `None` when nothing was pressed.
pub fn render_plan_offer(ui: &mut egui::Ui) -> PlanAction {
    let mut action = PlanAction::default();
    ui.add_space(6.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new(t("chat.plan_offer_note")).weak());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button(t("chat.plan_approve"))
                .on_hover_text(t("chat.plan_approve_hint"))
                .clicked()
            {
                action.approve = true;
            }
            if ui
                .button(t("chat.plan_discard"))
                .on_hover_text(t("chat.plan_discard_hint"))
                .clicked()
            {
                action.discard = true;
            }
        });
    });
    action
}

impl OctaApp {
    /// Draw the offer at the end of the transcript, if the last reply was one.
    ///
    /// Only once the turn has finished: a half-written plan is not something
    /// to approve.
    pub(crate) fn render_pending_plan(&mut self, ui: &mut egui::Ui) -> Option<PlanAction> {
        if !self.chat.plan_offer {
            return None;
        }
        let action = render_plan_offer(ui);
        (action != PlanAction::default()).then_some(action)
    }

    /// Act on the bar, with no lock held.
    pub(crate) fn apply_plan_action(&mut self, action: PlanAction, ctx: &egui::Context) {
        self.chat.plan_offer = false;
        if action.discard {
            self.status_message = Some((t("chat.plan_discarded"), std::time::Instant::now()));
            return;
        }
        if !action.approve {
            return;
        }
        // Switch to Data for the run. The mode is visible in the header, so
        // the gate being open is visible too - and the user flips it back when
        // they want the next thing planned first.
        self.chat.mode = ChatTurnMode::Data;
        self.chat.input = t("chat.plan_go_ahead");
        self.send_chat_message(ctx);
    }
}
