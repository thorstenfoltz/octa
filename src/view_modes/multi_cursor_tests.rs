use super::*;

#[test]
fn typing_at_every_caret_shifts_the_ones_after_it() {
    let (out, moved) = apply("ab\ncd\nef", &[0, 3, 6], &Edit::Insert("- "));
    assert_eq!(out, "- ab\n- cd\n- ef");
    assert_eq!(moved, [2, 7, 12]);
}

#[test]
fn backspace_and_delete_at_every_caret() {
    let (out, moved) = apply("xab\nxcd", &[1, 5], &Edit::Backspace);
    assert_eq!(out, "ab\ncd");
    assert_eq!(moved, [0, 3]);
    let (out, moved) = apply("ab\ncd", &[0, 3], &Edit::Delete);
    assert_eq!(out, "b\nd");
    assert_eq!(moved, [0, 2]);
    // At the very start nothing goes; two carets side by side both delete.
    assert_eq!(apply("abc", &[0], &Edit::Backspace).0, "abc");
    assert_eq!(apply("abc", &[2, 3], &Edit::Backspace).0, "a");
}

#[test]
fn a_paste_with_one_line_per_caret_puts_one_at_each() {
    let (out, _) = apply("a\nb", &[1, 3], &Edit::InsertEach(vec!["1", "2"]));
    assert_eq!(out, "a1\nb2");
}

#[test]
fn carets_move_by_line_and_keep_their_column() {
    let chars: Vec<char> = "abcd\nx\nabcd".chars().collect();
    assert_eq!(step(&chars, 3, Move::Down), 6, "clamped to the short line");
    assert_eq!(step(&chars, 6, Move::Down), 8);
    assert_eq!(step(&chars, 8, Move::Up), 6);
    assert_eq!(step(&chars, 2, Move::Up), 0, "above the first line");
    assert_eq!(step(&chars, 6, Move::Home), 5);
    assert_eq!(step(&chars, 0, Move::End), 4);
    assert_eq!(other_line(&chars, 3, 3, true), Some(6));
    assert_eq!(other_line(&chars, 9, 3, true), None, "below the last line");
}

#[test]
fn line_ends_of_a_selection() {
    let chars: Vec<char> = "ab\ncd\nef".chars().collect();
    assert_eq!(line_ends(&chars, 1, 7), [2, 5, 8]);
    // Ending at the start of a line leaves that line out.
    assert_eq!(line_ends(&chars, 0, 6), [2, 5]);
    assert_eq!(line_ends(&chars, 4, 4), [5]);
}

/// One frame of an editor wired the way the real ones are.
fn frame(ctx: &egui::Context, text: &mut String, events: Vec<egui::Event>) {
    frame_held(ctx, text, events, egui::Modifiers::NONE);
}

/// [`frame`] with `held` down, as a click needs.
fn frame_held(
    ctx: &egui::Context,
    text: &mut String,
    events: Vec<egui::Event>,
    held: egui::Modifiers,
) {
    // egui keeps what is held from these events, across frames.
    let events = std::iter::once(egui::Event::ModifiersChanged(held))
        .chain(events)
        .collect();
    let input = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(400.0, 300.0),
        )),
        ..Default::default()
    };
    let id = egui::Id::new("multi_cursor_test_editor");
    let mut out = ctx.run_ui(input, |ui| {
        let multi = MultiCursor::before(ui, id, text, "  ", true);
        let filter = event_filter(ui.ctx(), id);
        let mut output = egui::TextEdit::multiline(text)
            .id(id)
            .event_filter(filter)
            .show(ui);
        multi.after(ui, &mut output);
        // Once, as the editors do: a request every frame resets the focus
        // filter, and with it the claim on Escape.
        if !ui.memory(|m| m.has_focus(id)) {
            output.response.response.request_focus();
        }
    });
    out.textures_delta.clear();
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// A focused editor over three lines, its caret at the start.
fn editor_at_start(ctx: &egui::Context) -> String {
    let id = egui::Id::new("multi_cursor_test_editor");
    let mut text = String::from("one\ntwo\nthree");
    frame(ctx, &mut text, vec![]);
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(0))));
    state.store(ctx, id);
    frame(ctx, &mut text, vec![]);
    text
}

#[test]
fn shift_alt_down_then_typing_writes_on_every_line() {
    let ctx = egui::Context::default();
    let id = egui::Id::new("multi_cursor_test_editor");
    let mut text = editor_at_start(&ctx);

    let add = egui::Modifiers {
        alt: true,
        shift: true,
        ..Default::default()
    };
    frame(
        &ctx,
        &mut text,
        vec![
            key(egui::Key::ArrowDown, add),
            key(egui::Key::ArrowDown, add),
        ],
    );
    assert!(has_extras(&ctx, id));
    frame(&ctx, &mut text, vec![egui::Event::Text("- ".into())]);
    assert_eq!(text, "- one\n- two\n- three");

    // Backspace at every caret, then Escape is back to one caret.
    frame(
        &ctx,
        &mut text,
        vec![key(egui::Key::Backspace, egui::Modifiers::NONE)],
    );
    assert_eq!(text, "-one\n-two\n-three");
    frame(
        &ctx,
        &mut text,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!has_extras(&ctx, id));
    frame(&ctx, &mut text, vec![egui::Event::Text("x".into())]);
    assert_eq!(text, "-xone\n-two\n-three", "one caret types once");
}

#[test]
fn caret_keys_follow_the_settings() {
    let id = egui::Id::new("multi_cursor_test_editor");
    let shift_alt = egui::Modifiers {
        alt: true,
        shift: true,
        ..Default::default()
    };
    let mut shortcuts = Shortcuts::default();
    shortcuts.set(
        SA::AddCaretBelow,
        KeyCombo {
            key: Some(egui::Key::J),
            ctrl: false,
            shift: false,
            alt: true,
        },
    );
    let ctx = egui::Context::default();
    set_config(&ctx, CaretConfig::new(&shortcuts, CaretClick::Alt));
    let mut text = editor_at_start(&ctx);

    frame(&ctx, &mut text, vec![key(egui::Key::ArrowDown, shift_alt)]);
    assert!(!has_extras(&ctx, id), "the old key no longer adds one");
    frame(
        &ctx,
        &mut text,
        vec![key(egui::Key::J, egui::Modifiers::ALT)],
    );
    assert!(has_extras(&ctx, id), "the new key does");

    // Unbound, the line-ends key does nothing at all.
    shortcuts.set(SA::CaretsAtLineEnds, KeyCombo::UNBOUND);
    let ctx = egui::Context::default();
    set_config(&ctx, CaretConfig::new(&shortcuts, CaretClick::Alt));
    let mut text = editor_at_start(&ctx);
    frame(&ctx, &mut text, vec![key(egui::Key::I, shift_alt)]);
    assert!(!has_extras(&ctx, id));
}

/// Clicks on the second line with `held` down, the caret on the first.
fn click_adds_a_caret(click: Option<CaretClick>, held: egui::Modifiers) -> bool {
    let ctx = egui::Context::default();
    if let Some(click) = click {
        set_config(&ctx, CaretConfig::new(&Shortcuts::default(), click));
    }
    let mut text = editor_at_start(&ctx);
    let pos = egui::pos2(20.0, 26.0);
    let press = egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: held,
    };
    frame_held(
        &ctx,
        &mut text,
        vec![egui::Event::PointerMoved(pos), press],
        held,
    );
    has_extras(&ctx, egui::Id::new("multi_cursor_test_editor"))
}

#[test]
fn the_caret_click_follows_the_settings() {
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let alt = egui::Modifiers::ALT;
    assert!(click_adds_a_caret(None, alt), "Alt+click by default");
    assert!(!click_adds_a_caret(None, ctrl));
    assert!(click_adds_a_caret(Some(CaretClick::Ctrl), ctrl));
    assert!(!click_adds_a_caret(Some(CaretClick::Ctrl), alt));
    assert!(!click_adds_a_caret(Some(CaretClick::Off), alt));
    assert!(!click_adds_a_caret(Some(CaretClick::Off), ctrl));
}
