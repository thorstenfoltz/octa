//! The bar under a writing-mode reply: show the proposed text beside the
//! current one, accept or discard it, or take an accepted change back.
//!
//! Interaction-struct shaped for the same reason as `plan_ui`: the transcript
//! holds the chat session lock while it draws. The renderer reports; `OctaApp`
//! acts afterwards.
//!
//! Nothing here writes a file. A proposal becomes an unsaved change in
//! `TabState.raw_content`, and only while that text is still the text the
//! question was asked about.

use egui::RichText;
use octa::i18n::t;

use crate::app::chat::types::{ContentBlock, Role};
use crate::app::chat::writing;
use crate::app::state::OctaApp;
use crate::app::tabs::TabId;

/// The tab a writing turn was asked about and the text it was sent.
#[derive(Debug, Clone, PartialEq)]
pub struct WritingSent {
    pub tab: TabId,
    pub base: String,
}

/// A proposed new text for a tab, waiting for the user's decision.
#[derive(Debug, Clone, PartialEq)]
pub struct WritingOffer {
    pub tab: TabId,
    /// The text the question was asked about. The proposal only goes in while
    /// the tab still holds exactly this.
    pub base: String,
    pub proposal: String,
    /// The view the tab was in before the comparison opened; `None` while the
    /// comparison is not showing.
    pub shown_from: Option<octa::data::ViewMode>,
}

/// The last accepted change, so one button can take it back. The Raw view's
/// Ctrl+Z belongs to egui's text field and does not see text set from outside.
#[derive(Debug, Clone, PartialEq)]
pub struct WritingUndo {
    pub tab: TabId,
    pub before: String,
    pub after: String,
}

/// What the user pressed this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WritingAction {
    pub show: bool,
    pub accept: bool,
    pub discard: bool,
    pub undo: bool,
}

impl OctaApp {
    /// Turn the finished reply of a writing turn into an offer, once.
    ///
    /// Called every frame while no turn runs; it does nothing unless a
    /// writing turn was sent and not yet collected.
    pub(crate) fn collect_writing_offer(&mut self) {
        let Some(sent) = self.chat.writing_sent.take() else {
            return;
        };
        let reply = {
            let guard = self.chat.session.lock().unwrap();
            guard
                .messages
                .last()
                .filter(|m| matches!(m.role, Role::Assistant))
                .map(|m| {
                    m.blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
        };
        // No assistant message last: the turn failed or was cancelled.
        let Some(proposal) = reply.as_deref().and_then(writing::proposal_in) else {
            return;
        };
        let proposal = writing::with_ending_of(&sent.base, proposal);
        if proposal == sent.base {
            return;
        }
        // A newer proposal replaces an undecided one; close its comparison.
        self.close_writing_comparison();
        self.chat.writing_offer = Some(WritingOffer {
            tab: sent.tab,
            base: sent.base,
            proposal,
            shown_from: None,
        });
        // "Apply writing changes directly": no question, the text goes in and
        // Undo change takes it back. Where it may not go in (Read-only mode,
        // a profile without writes) the offer stays, with the reason on Accept.
        if self.settings.chat_writing_apply_directly && self.writing_blocked().is_none() {
            self.accept_writing_offer();
        }
    }

    /// The user closed the comparison with the view's own button. The view
    /// has cleared its right side; put the tab back where the comparison was
    /// opened from. The offer itself stays, so Accept and Discard still work.
    pub(crate) fn writing_comparison_closed(&mut self, tab_idx: usize) {
        let Some(offer) = &mut self.chat.writing_offer else {
            return;
        };
        if self.tabs.get(tab_idx).map(|t| t.id) != Some(offer.tab) {
            return;
        }
        if let Some(from) = offer.shown_from.take() {
            self.tabs[tab_idx].view_mode = from;
        }
    }

    /// Draw the bar at the end of the transcript.
    pub(crate) fn render_writing_bar(&mut self, ui: &mut egui::Ui) -> Option<WritingAction> {
        let mut action = WritingAction::default();
        let blocked = self.writing_blocked();

        if self.chat.writing_offer.is_some() {
            ui.add_space(6.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(t("chat.writing_offer_note")).weak());
                ui.add_space(4.0);
                // Always all three, so the choice never depends on what was
                // pressed before.
                octa::ui::control_row::control_line(ui, |ui| {
                    if ui
                        .button(t("dialog.versions_compare"))
                        .on_hover_text(t("chat.writing_show_hint"))
                        .clicked()
                    {
                        action.show = true;
                    }
                    // The one button that changes the text. Greyed with the
                    // reason while the profile or Read-only mode forbids it.
                    let hint = t("chat.writing_accept_hint");
                    let resp = ui.add_enabled(
                        blocked.is_none(),
                        egui::Button::new(t("chat.writing_accept")),
                    );
                    match blocked {
                        Some(why) => {
                            resp.on_disabled_hover_text(octa::ui::menu_entry::disabled_tip(
                                &hint,
                                &t(why),
                            ));
                        }
                        None => {
                            if resp.on_hover_text(hint).clicked() {
                                action.accept = true;
                            }
                        }
                    }
                    if ui
                        .button(t("chat.plan_discard"))
                        .on_hover_text(t("chat.writing_discard_hint"))
                        .clicked()
                    {
                        action.discard = true;
                    }
                });
            });
        } else if self.chat.writing_undo.is_some() {
            ui.add_space(6.0);
            octa::ui::control_row::control_line(ui, |ui| {
                if ui
                    .button(t("chat.writing_undo"))
                    .on_hover_text(t("chat.writing_undo_hint"))
                    .clicked()
                {
                    action.undo = true;
                }
            });
        }
        (action != WritingAction::default()).then_some(action)
    }

    /// Act on the bar, with no lock held.
    pub(crate) fn apply_writing_action(&mut self, action: WritingAction) {
        if action.undo {
            self.undo_writing_change();
        } else if action.discard {
            self.close_writing_comparison();
            self.chat.writing_offer = None;
        } else if action.show {
            self.show_writing_comparison();
        } else if action.accept {
            self.accept_writing_offer();
        }
    }

    /// A new or loaded session starts with no pending proposal.
    pub(crate) fn drop_writing_offer(&mut self) {
        self.close_writing_comparison();
        self.chat.writing_sent = None;
        self.chat.writing_offer = None;
        self.chat.writing_undo = None;
    }

    fn writing_status(&mut self, key: &str) {
        self.status_message = Some((t(key), std::time::Instant::now()));
    }

    fn writing_tab(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// Put the proposal on the right of the existing Text Diff view.
    fn show_writing_comparison(&mut self) {
        let Some(offer) = self.chat.writing_offer.clone() else {
            return;
        };
        let Some(idx) = self.writing_tab(offer.tab) else {
            self.chat.writing_offer = None;
            self.writing_status("chat.writing_stale");
            return;
        };
        // Pressed again while it is showing: it is already there.
        if offer.shown_from.is_some() {
            self.active_tab = idx;
            return;
        }
        let tab = &mut self.tabs[idx];
        let from = tab.view_mode;
        tab.compare_error = None;
        // The compare view asks for a file while this is `None`; the label is
        // what it shows as the right side's name.
        tab.compare_right_path = Some(std::path::PathBuf::from(t("chat.writing_proposal_label")));
        tab.compare_right_raw = Some(offer.proposal);
        tab.compare_right_table = None;
        tab.compare_cache = None;
        tab.compare_columns_left.clear();
        tab.compare_columns_right.clear();
        tab.compare_mode = octa::data::CompareMode::TextDiff;
        tab.view_mode = octa::data::ViewMode::Compare;
        self.active_tab = idx;
        if let Some(o) = &mut self.chat.writing_offer {
            o.shown_from = Some(from);
        }
    }

    /// Leave the comparison the offer opened, if it is showing.
    fn close_writing_comparison(&mut self) {
        let Some(offer) = &mut self.chat.writing_offer else {
            return;
        };
        let Some(from) = offer.shown_from.take() else {
            return;
        };
        let id = offer.tab;
        let proposal = offer.proposal.clone();
        if let Some(idx) = self.writing_tab(id) {
            let tab = &mut self.tabs[idx];
            // Only if the tab still shows this proposal: the user may have
            // left the comparison or started another one meanwhile.
            if tab.view_mode == octa::data::ViewMode::Compare
                && tab.compare_right_raw.as_deref() == Some(proposal.as_str())
            {
                tab.compare_right_path = None;
                tab.compare_right_raw = None;
                tab.compare_cache = None;
                tab.view_mode = from;
            }
        }
    }

    fn accept_writing_offer(&mut self) {
        // Read-only mode and a read-only profile keep the offer: the user can
        // flip the switch and accept then.
        if self.writing_blocked().is_some() {
            return;
        }
        let Some(offer) = self.chat.writing_offer.clone() else {
            return;
        };
        self.close_writing_comparison();
        self.chat.writing_offer = None;
        let Some(idx) = self.writing_tab(offer.tab) else {
            self.writing_status("chat.writing_stale");
            return;
        };
        let tab = &mut self.tabs[idx];
        if tab.raw_content.as_deref() != Some(offer.base.as_str()) {
            self.writing_status("chat.writing_stale");
            return;
        }
        tab.raw_content = Some(offer.proposal.clone());
        // A tree view shows the parsed text: it has to read back, or the tab
        // would show one thing and save another.
        if let Err(e) = tab.refresh_tree_from_text() {
            tab.raw_content = Some(offer.base);
            self.status_message = Some((
                t("chat.writing_breaks_text").replace("{error}", &e),
                std::time::Instant::now(),
            ));
            return;
        }
        tab.raw_content_modified = true;
        tab.markdown_render_cache = None;
        self.chat.writing_undo = Some(WritingUndo {
            tab: offer.tab,
            before: offer.base,
            after: offer.proposal,
        });
        self.writing_status("chat.writing_applied");
    }

    fn undo_writing_change(&mut self) {
        if self.readonly_mode {
            return;
        }
        let Some(undo) = self.chat.writing_undo.take() else {
            return;
        };
        let Some(idx) = self.writing_tab(undo.tab) else {
            return;
        };
        let tab = &mut self.tabs[idx];
        // Only while the text is still the accepted version: anything typed
        // after it would be lost.
        if tab.raw_content.as_deref() != Some(undo.after.as_str()) {
            self.writing_status("chat.writing_stale");
            return;
        }
        tab.raw_content = Some(undo.before);
        if let Err(e) = tab.refresh_tree_from_text() {
            tab.raw_content = Some(undo.after);
            self.status_message = Some((
                t("chat.writing_breaks_text").replace("{error}", &e),
                std::time::Instant::now(),
            ));
            return;
        }
        tab.raw_content_modified = true;
        tab.markdown_render_cache = None;
    }
}

#[cfg(test)]
#[path = "writing_ui_tests.rs"]
mod tests;
