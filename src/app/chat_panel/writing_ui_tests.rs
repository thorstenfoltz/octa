use super::*;
use crate::app::chat::types::Message;
use crate::app::chat::writing::{CLOSE_FENCE, OPEN_FENCE};

fn app_with_text(text: &str) -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.settings.show_readonly_notice = false;
    octa::ui::settings::chat_profiles::ensure_profiles(&mut app.settings);
    for p in &mut app.settings.chat_profiles {
        p.allow_writes = true;
    }
    app.tabs[0].raw_content = Some(text.into());
    app.tabs[0].view_mode = octa::data::ViewMode::Raw;
    app
}

/// Put a finished assistant reply into the session, as the worker would.
fn reply(app: &mut OctaApp, base: &str, text: &str) {
    app.chat.writing_sent = Some(WritingSent {
        tab: app.tabs[0].id,
        base: base.into(),
    });
    app.chat.session.lock().unwrap().messages.push(Message {
        role: Role::Assistant,
        blocks: vec![ContentBlock::text(text)],
    });
    app.collect_writing_offer();
}

fn proposal(text: &str) -> String {
    format!("Changed.\n{OPEN_FENCE}\n{text}\n{CLOSE_FENCE}")
}

fn press(app: &mut OctaApp, action: WritingAction) {
    app.apply_writing_action(action);
}

const SHOW: WritingAction = WritingAction {
    show: true,
    accept: false,
    discard: false,
    undo: false,
};
const ACCEPT: WritingAction = WritingAction {
    show: false,
    accept: true,
    discard: false,
    undo: false,
};
const DISCARD: WritingAction = WritingAction {
    show: false,
    accept: false,
    discard: true,
    undo: false,
};
const UNDO: WritingAction = WritingAction {
    show: false,
    accept: false,
    discard: false,
    undo: true,
};

#[test]
fn a_reply_with_a_block_becomes_an_offer() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    let offer = app.chat.writing_offer.as_ref().expect("an offer");
    assert_eq!(
        offer.proposal, "new\n",
        "the file's last line break is kept"
    );
    assert!(app.chat.writing_sent.is_none(), "collected once");
}

#[test]
fn a_plain_answer_offers_nothing() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", "It is a note.");
    assert!(app.chat.writing_offer.is_none());
}

#[test]
fn a_proposal_equal_to_the_text_offers_nothing() {
    let mut app = app_with_text("same\n");
    reply(&mut app, "same\n", &proposal("same"));
    assert!(app.chat.writing_offer.is_none());
}

#[test]
fn a_failed_turn_offers_nothing() {
    // The last message is the user's own: no reply arrived.
    let mut app = app_with_text("old\n");
    app.chat.writing_sent = Some(WritingSent {
        tab: app.tabs[0].id,
        base: "old\n".into(),
    });
    app.chat
        .session
        .lock()
        .unwrap()
        .messages
        .push(Message::user_text(proposal("new")));
    app.collect_writing_offer();
    assert!(app.chat.writing_offer.is_none());
}

#[test]
fn show_opens_the_comparison_and_discard_restores_the_view() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    press(&mut app, SHOW);
    assert_eq!(app.tabs[0].view_mode, octa::data::ViewMode::Compare);
    assert_eq!(app.tabs[0].compare_right_raw.as_deref(), Some("new\n"));
    assert_eq!(
        app.tabs[0].raw_content.as_deref(),
        Some("old\n"),
        "nothing changed yet"
    );

    press(&mut app, DISCARD);
    assert_eq!(app.tabs[0].view_mode, octa::data::ViewMode::Raw);
    assert!(app.tabs[0].compare_right_raw.is_none());
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("old\n"));
    assert!(app.chat.writing_offer.is_none());
}

#[test]
fn accept_puts_the_text_in_as_an_unsaved_change() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    press(&mut app, SHOW);
    press(&mut app, ACCEPT);
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("new\n"));
    assert!(app.tabs[0].raw_content_modified);
    assert_eq!(
        app.tabs[0].view_mode,
        octa::data::ViewMode::Raw,
        "back from the comparison"
    );
    assert!(app.tabs[0].compare_right_raw.is_none());
    assert!(app.chat.writing_offer.is_none());
    assert!(app.chat.writing_undo.is_some());
}

#[test]
fn undo_puts_the_old_text_back() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    press(&mut app, ACCEPT);
    press(&mut app, UNDO);
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("old\n"));
    assert!(app.chat.writing_undo.is_none());
}

#[test]
fn undo_is_refused_once_the_user_typed_on() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    press(&mut app, ACCEPT);
    app.tabs[0].raw_content = Some("new\nand more\n".into());
    press(&mut app, UNDO);
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("new\nand more\n"));
}

#[test]
fn a_proposal_is_never_put_over_text_that_changed_since_the_question() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    app.tabs[0].raw_content = Some("old, edited meanwhile\n".into());
    press(&mut app, ACCEPT);
    assert_eq!(
        app.tabs[0].raw_content.as_deref(),
        Some("old, edited meanwhile\n")
    );
    assert!(
        app.chat.writing_offer.is_none(),
        "the stale offer is dropped"
    );
    assert!(app.chat.writing_undo.is_none());
}

#[test]
fn read_only_mode_refuses_accept_and_keeps_the_offer() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    app.readonly_mode = true;
    press(&mut app, ACCEPT);
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("old\n"));
    assert!(
        app.chat.writing_offer.is_some(),
        "the offer waits for the switch"
    );
}

#[test]
fn a_profile_without_writes_refuses_accept() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    for p in &mut app.settings.chat_profiles {
        p.allow_writes = false;
    }
    press(&mut app, ACCEPT);
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("old\n"));
}

#[test]
fn a_closed_tab_drops_the_offer() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    app.chat.writing_offer.as_mut().unwrap().tab = TabId::fresh();
    press(&mut app, ACCEPT);
    assert!(app.chat.writing_offer.is_none());
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("old\n"));
}

#[test]
fn windows_line_endings_survive_an_accepted_change() {
    let mut app = app_with_text("a\r\nb\r\n");
    reply(&mut app, "a\r\nb\r\n", &proposal("a\nc"));
    press(&mut app, ACCEPT);
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("a\r\nc\r\n"));
}

#[test]
fn a_new_session_drops_a_pending_proposal_and_its_comparison() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    press(&mut app, SHOW);
    app.new_chat_session();
    assert!(app.chat.writing_offer.is_none());
    assert_eq!(app.tabs[0].view_mode, octa::data::ViewMode::Raw);
}

#[test]
fn apply_directly_changes_the_text_without_asking() {
    let mut app = app_with_text("old\n");
    app.settings.chat_writing_apply_directly = true;
    reply(&mut app, "old\n", &proposal("new"));
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("new\n"));
    assert!(app.chat.writing_offer.is_none(), "nothing left to answer");
    assert!(app.chat.writing_undo.is_some(), "and it can be taken back");
}

#[test]
fn apply_directly_still_asks_where_it_may_not_write() {
    let mut app = app_with_text("old\n");
    app.settings.chat_writing_apply_directly = true;
    app.readonly_mode = true;
    reply(&mut app, "old\n", &proposal("new"));
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("old\n"));
    assert!(app.chat.writing_offer.is_some());
}

#[test]
fn closing_the_comparison_by_hand_returns_to_the_view_it_came_from() {
    let mut app = app_with_text("old\n");
    app.tabs[0].view_mode = octa::data::ViewMode::Markdown;
    reply(&mut app, "old\n", &proposal("new"));
    press(&mut app, SHOW);
    // What the compare view's own close button does.
    app.tabs[0].compare_right_path = None;
    app.tabs[0].compare_right_raw = None;
    app.tabs[0].view_mode = octa::data::ViewMode::Table;
    app.writing_comparison_closed(0);
    assert_eq!(app.tabs[0].view_mode, octa::data::ViewMode::Markdown);
    assert!(
        app.chat.writing_offer.is_some(),
        "the proposal is still on offer"
    );
    // And it can still be accepted from there.
    press(&mut app, ACCEPT);
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("new\n"));
    assert_eq!(app.tabs[0].view_mode, octa::data::ViewMode::Markdown);
}

#[test]
fn compare_pressed_twice_keeps_the_view_to_return_to() {
    let mut app = app_with_text("old\n");
    reply(&mut app, "old\n", &proposal("new"));
    press(&mut app, SHOW);
    press(&mut app, SHOW);
    press(&mut app, DISCARD);
    assert_eq!(app.tabs[0].view_mode, octa::data::ViewMode::Raw);
}

#[test]
fn an_empty_file_takes_a_proposal() {
    let mut app = app_with_text("");
    reply(&mut app, "", &proposal("# Title"));
    press(&mut app, ACCEPT);
    assert_eq!(app.tabs[0].raw_content.as_deref(), Some("# Title"));
}
