use super::*;

const W: f32 = 900.0;

fn texts() -> FindBoxTexts {
    FindBoxTexts {
        placeholder: "Find".into(),
        find_hint: String::new(),
        replace_all_hint: String::new(),
    }
}

fn base() -> egui::Id {
    egui::Id::new("find_box_test")
}

/// One frame of the box in a `W` wide window, its corner at the top right.
fn frame(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    input: &mut FindInput,
    count: usize,
    block: Option<&str>,
) -> FindBoxOut {
    frame_in(ctx, events, input, count, block, W)
}

/// [`frame`] in a view `width` wide.
fn frame_in(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    input: &mut FindInput,
    count: usize,
    block: Option<&str>,
    width: f32,
) -> FindBoxOut {
    // egui reads Shift from the held modifiers, which it tracks from
    // `ModifiersChanged`, not from the key event.
    let held = events
        .iter()
        .find_map(|e| match e {
            egui::Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    let mut events = events;
    events.insert(0, egui::Event::ModifiersChanged(held));
    let raw = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, 400.0),
        )),
        ..Default::default()
    };
    let mut out = FindBoxOut::default();
    let mut full = ctx.run_ui(raw, |ui| {
        out = draw(
            ui,
            egui::pos2(width, 0.0),
            base(),
            input,
            (count, 0),
            block,
            &texts(),
        );
    });
    full.textures_delta.clear();
    out
}

fn key(key: egui::Key, shift: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: if shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        },
    }
}

/// The SQL editor's strip squeezed its find field to a sliver and laid the
/// buttons over it. The box keeps the field whole, puts the replace field
/// under it (VS Code's layout) and stays inside the corner it was given.
#[test]
fn the_replace_field_sits_under_a_whole_find_field_inside_the_corner() {
    let ctx = egui::Context::default();
    let mut input = FindInput {
        query: "amount".into(),
        replacing: true,
        ..Default::default()
    };
    for _ in 0..3 {
        frame(&ctx, Vec::new(), &mut input, 2, None);
    }
    let find = ctx.read_response(find_field_id(base())).unwrap().rect;
    let replace = ctx.read_response(replace_field_id(base())).unwrap().rect;
    assert!(
        find.width() >= FIELD_WIDTH - 1.0,
        "find field squeezed: {find:?}"
    );
    assert!(
        replace.top() >= find.bottom(),
        "{replace:?} not under {find:?}"
    );
    assert!((replace.left() - find.left()).abs() < 0.5, "not aligned");
    let whole = ctx.read_response(base().with("find_box_bg")).unwrap().rect;
    assert!(whole.contains_rect(find) && whole.contains_rect(replace));
    assert!(
        whole.right() <= W - RIGHT_GAP + 0.5,
        "the box runs past its corner: {whole:?}"
    );
}

#[test]
fn enter_steps_escape_closes_and_enter_in_replace_replaces() {
    let ctx = egui::Context::default();
    let mut input = FindInput {
        query: "a".into(),
        replacing: true,
        focus: true,
        ..Default::default()
    };
    for _ in 0..2 {
        frame(&ctx, Vec::new(), &mut input, 3, None);
    }
    assert_eq!(ctx.memory(|m| m.focused()), Some(find_field_id(base())));
    assert!(
        frame(
            &ctx,
            vec![key(egui::Key::Enter, false)],
            &mut input,
            3,
            None
        )
        .next
    );
    frame(&ctx, Vec::new(), &mut input, 3, None);
    assert!(frame(&ctx, vec![key(egui::Key::Enter, true)], &mut input, 3, None).prev);
    frame(&ctx, Vec::new(), &mut input, 3, None);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(find_field_id(base())),
        "Enter keeps the keyboard in the find field"
    );

    ctx.memory_mut(|m| m.request_focus(replace_field_id(base())));
    frame(&ctx, Vec::new(), &mut input, 3, None);
    assert!(
        frame(
            &ctx,
            vec![key(egui::Key::Enter, false)],
            &mut input,
            3,
            None
        )
        .replace_one
    );
    frame(&ctx, Vec::new(), &mut input, 3, None);
    // Read-only: Enter in the replace field replaces nothing.
    let out = frame(
        &ctx,
        vec![key(egui::Key::Enter, false)],
        &mut input,
        3,
        Some("read-only"),
    );
    assert!(!out.replace_one);
    frame(&ctx, Vec::new(), &mut input, 3, None);
    assert!(
        frame(
            &ctx,
            vec![key(egui::Key::Escape, false)],
            &mut input,
            3,
            None
        )
        .close
    );
}

#[test]
fn the_switches_make_the_match_strict_or_a_pattern() {
    let mut input = FindInput {
        query: "id".into(),
        ..Default::default()
    };
    assert!(matcher(&input).matches("ID"), "any case by default");
    input.case_sensitive = true;
    assert!(!matcher(&input).matches("ID"));
    input.case_sensitive = false;
    input.whole_word = true;
    assert!(!matcher(&input).matches("idea"));
    assert!(matcher(&input).matches("an id"));

    let regex = FindInput {
        query: r"(\w+)@x".into(),
        regex: true,
        ..Default::default()
    };
    assert_eq!(matcher(&regex).replace_all("bob@x", "$1"), "bob");
    let plain = FindInput {
        query: "a".into(),
        ..Default::default()
    };
    assert_eq!(
        matcher(&plain).replace_all("a", "$1"),
        "$1",
        "plain text goes in as typed"
    );
}

/// In a narrow view (an SQL editor docked at the side, several editors side
/// by side) the fields shrink so the box still fits.
#[test]
fn in_a_narrow_view_the_fields_shrink_to_fit() {
    let ctx = egui::Context::default();
    let mut input = FindInput {
        query: "a".into(),
        replacing: true,
        ..Default::default()
    };
    let narrow = 400.0;
    for _ in 0..4 {
        frame_in(&ctx, Vec::new(), &mut input, 1, None, narrow);
    }
    let find = ctx.read_response(find_field_id(base())).unwrap().rect;
    let whole = ctx.read_response(base().with("find_box_bg")).unwrap().rect;
    assert!(find.width() < FIELD_WIDTH, "did not shrink: {find:?}");
    assert!(
        whole.left() >= 0.0 && whole.right() <= narrow - RIGHT_GAP + 0.5,
        "{whole:?}"
    );
}

/// Every rectangle and text the box paints, in order.
fn geometry(ctx: &egui::Context, input: &mut FindInput, pointer: egui::Pos2) -> Vec<egui::Rect> {
    let raw = egui::RawInput {
        events: vec![egui::Event::PointerMoved(pointer)],
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(W, 400.0),
        )),
        ..Default::default()
    };
    let mut full = ctx.run_ui(raw, |ui| {
        draw(
            ui,
            egui::pos2(W, 0.0),
            base(),
            input,
            (3, 0),
            None,
            &texts(),
        );
    });
    full.textures_delta.clear();
    full.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r) => Some(r.rect),
            egui::Shape::Text(t) => Some(t.visual_bounding_rect()),
            _ => None,
        })
        // An empty text (a field's caret line, the blank count) has no extent.
        .filter(|r| r.is_finite())
        .collect()
}

/// Moving the pointer over the box changes colours, never sizes: themes
/// paint a hovered widget up to 3px larger, and over a box this small that
/// reads as twitching.
#[test]
fn hovering_the_box_moves_and_grows_nothing() {
    let ctx = egui::Context::default();
    ctx.global_style_mut(|s| {
        s.visuals.widgets.hovered.expansion = 3.0;
        s.interaction.tooltip_delay = 1000.0;
    });
    let mut input = FindInput {
        query: "a".into(),
        replacing: true,
        ..Default::default()
    };
    let away = egui::pos2(5.0, 300.0);
    for _ in 0..3 {
        geometry(&ctx, &mut input, away);
    }
    let still = geometry(&ctx, &mut input, away);
    let field = ctx.read_response(find_field_id(base())).unwrap().rect;
    let replace = ctx.read_response(replace_field_id(base())).unwrap().rect;
    for over in [
        field.center(),
        replace.center(),
        // The buttons at the end of each row.
        egui::pos2(W - RIGHT_GAP - 20.0, field.center().y),
        egui::pos2(W - RIGHT_GAP - 20.0, replace.center().y),
    ] {
        geometry(&ctx, &mut input, over);
        assert_eq!(geometry(&ctx, &mut input, over), still, "hovering {over:?}");
    }
}
