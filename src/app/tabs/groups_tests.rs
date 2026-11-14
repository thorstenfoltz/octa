use super::*;

const A: Option<GroupId> = Some(GroupId(1));
const B: Option<GroupId> = Some(GroupId(2));

#[test]
fn members_are_pulled_to_their_first_member() {
    // [x a y a b x b] -> a a at 1, then y, b b, x x keep order.
    assert_eq!(
        contiguous_order(&[None, A, None, A, B, None, B]),
        [0, 1, 3, 2, 4, 6, 5]
    );
    assert_eq!(contiguous_order(&[A, A, None]), [0, 1, 2]);
}

#[test]
fn a_drop_inside_a_group_joins_it() {
    assert_eq!(group_after_drop(None, A, A), A);
    assert_eq!(group_after_drop(None, A, None), None);
    assert_eq!(group_after_drop(None, None, A), None);
    assert_eq!(group_after_drop(None, A, B), None);
}

#[test]
fn a_member_stays_while_a_neighbour_is_in_its_group() {
    assert_eq!(group_after_drop(A, A, None), A);
    assert_eq!(group_after_drop(A, None, A), A);
    assert_eq!(group_after_drop(A, None, None), None);
    assert_eq!(group_after_drop(A, B, B), B);
}

#[test]
fn a_chip_slot_inside_another_group_snaps_past_it() {
    // others: x a a b
    let o = [None, A, A, B];
    assert_eq!(snap_past_group(&o, 0), 0);
    assert_eq!(snap_past_group(&o, 1), 1);
    assert_eq!(snap_past_group(&o, 2), 3);
    assert_eq!(snap_past_group(&o, 3), 3);
    assert_eq!(snap_past_group(&[A, A], 1), 2);
    assert_eq!(snap_past_group(&o, 4), 4);
}

#[test]
fn the_default_colour_is_the_first_unused() {
    use octa::data::MarkColor;
    let g = |c| TabGroup {
        id: GroupId(9),
        name: String::new(),
        colour: c,
        folded: false,
    };
    assert_eq!(default_group_colour(&[]), MarkColor::ALL[0]);
    assert_eq!(
        default_group_colour(&[g(MarkColor::ALL[0]), g(MarkColor::ALL[2])]),
        MarkColor::ALL[1]
    );
    let all: Vec<_> = MarkColor::ALL.iter().map(|&c| g(c)).collect();
    assert_eq!(default_group_colour(&all), MarkColor::ALL[0]);
}

#[test]
fn new_groups_take_the_smallest_free_number() {
    let g = |n: &str| TabGroup {
        id: GroupId(9),
        name: n.into(),
        colour: octa::data::MarkColor::Blue,
        folded: false,
    };
    assert_eq!(next_group_name(&[], "Group"), "Group 1");
    assert_eq!(
        next_group_name(&[g("Group 1"), g("Group 3")], "Group"),
        "Group 2"
    );
}

#[test]
fn normalise_groups_gathers_members_and_drops_empty_groups() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    while app.tabs.len() < 4 {
        app.tabs
            .push(TabState::new(app.settings.default_search_mode));
    }
    let ids: Vec<_> = app.tabs.iter().map(|t| t.id).collect();
    let g = |n: u64| TabGroup {
        id: GroupId(n),
        name: format!("Group {n}"),
        colour: octa::data::MarkColor::Blue,
        folded: false,
    };
    app.tab_groups = vec![g(1), g(2)];
    app.tabs[0].group = A;
    app.tabs[2].group = A; // [a x a y], group 2 has no member
    app.normalise_groups();
    assert_eq!(
        app.tabs.iter().map(|t| t.id).collect::<Vec<_>>(),
        [ids[0], ids[2], ids[1], ids[3]]
    );
    assert_eq!(app.tab_groups, [g(1)]);
}

/// Tabs named `names`, the first one active, no groups.
fn app_with(names: &[&str]) -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs.clear();
    for name in names {
        let mut t = TabState::new(app.settings.default_search_mode);
        t.raw_content = Some(name.to_string());
        app.tabs.push(t);
    }
    app.active_tab = 0;
    app
}

fn names(app: &OctaApp) -> Vec<String> {
    app.tabs
        .iter()
        .map(|t| t.raw_content.clone().unwrap())
        .collect()
}

fn groups(app: &OctaApp) -> Vec<Option<GroupId>> {
    app.tabs.iter().map(|t| t.group).collect()
}

#[test]
fn a_new_group_gathers_its_members_at_the_first() {
    let mut app = app_with(&["a", "b", "c", "d"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[1, 3]);
    assert_eq!(names(&app), ["a", "b", "d", "c"]);
    assert_eq!(groups(&app), [None, Some(g), Some(g), None]);
    assert_eq!(app.tab_groups.len(), 1);
}

#[test]
fn joining_puts_the_tab_at_the_end_of_the_group() {
    let mut app = app_with(&["a", "b", "c", "d"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[1, 2]);
    app.add_to_group(0, g);
    assert_eq!(names(&app), ["b", "c", "a", "d"]);
    assert_eq!(groups(&app), [Some(g), Some(g), Some(g), None]);
    app.add_to_group(3, g);
    assert_eq!(names(&app), ["b", "c", "a", "d"]);
    assert_eq!(groups(&app), [Some(g); 4]);
}

#[test]
fn leaving_puts_the_tab_right_after_the_group() {
    let mut app = app_with(&["a", "b", "c", "d"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1, 2]);
    app.remove_from_group(0);
    assert_eq!(names(&app), ["b", "c", "a", "d"]);
    assert_eq!(groups(&app), [Some(g), Some(g), None, None]);
}

#[test]
fn ungroup_keeps_the_tabs_and_drops_the_group() {
    let mut app = app_with(&["a", "b", "c"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1]);
    app.ungroup(g);
    assert_eq!(names(&app), ["a", "b", "c"]);
    assert_eq!(groups(&app), [None, None, None]);
    assert!(app.tab_groups.is_empty());
}

#[test]
fn close_group_closes_its_unmodified_tabs_and_asks_for_the_others() {
    let mut app = app_with(&["a", "b", "c", "d"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[1, 2, 3]);
    app.tabs[2].raw_content_modified = true;
    app.tabs[3].raw_content_modified = true;
    app.close_group(g);
    assert_eq!(names(&app), ["a", "c", "d"]);
    assert_eq!(app.pending_close_tab, Some(1));
    assert!(app.show_close_confirm);
    // "Don't save" on the first prompt: the next one comes up.
    app.close_tab(1);
    app.show_close_confirm = false;
    app.pending_close_tab = None;
    app.next_queued_close();
    assert_eq!(app.pending_close_tab, Some(1));
    assert_eq!(names(&app)[1], "d");
}

#[test]
fn folding_moves_the_active_tab_out_of_the_group() {
    let mut app = app_with(&["a", "b", "c"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1]);
    app.active_tab = 1;
    app.toggle_fold(g);
    assert!(app.tab_groups[0].folded);
    assert_eq!(app.active_tab, 2);
    app.toggle_fold(g);
    assert!(!app.tab_groups[0].folded);
}

#[test]
fn folding_is_refused_when_every_tab_is_in_the_group() {
    let mut app = app_with(&["a", "b"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1]);
    app.toggle_fold(g);
    assert!(!app.tab_groups[0].folded);
    assert_eq!(
        app.status_message.as_ref().map(|(m, _)| m.clone()),
        Some(octa::i18n::t("greyed.fold_all_tabs"))
    );
}

#[test]
fn an_edge_member_released_in_place_keeps_its_group() {
    let mut app = app_with(&["a", "b", "c", "d"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1]);
    app.drop_tab(0, 0);
    app.drop_tab(1, 1);
    assert_eq!(groups(&app), [Some(g), Some(g), None, None]);
}

#[test]
fn a_two_member_group_can_be_reordered() {
    let mut app = app_with(&["a", "b", "c"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1]);
    app.drop_tab(0, 1);
    assert_eq!(names(&app), ["b", "a", "c"]);
    assert_eq!(groups(&app), [Some(g), Some(g), None]);
}

#[test]
fn moving_a_member_past_its_group_end_keeps_it() {
    let mut app = app_with(&["x", "a", "b", "c", "y"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[1, 2, 3]);
    app.drop_tab(1, 3);
    assert_eq!(names(&app), ["x", "b", "c", "a", "y"]);
    assert_eq!(groups(&app), [None, Some(g), Some(g), Some(g), None]);
}

#[test]
fn moving_an_edge_member_past_a_non_member_leaves_the_group() {
    let mut app = app_with(&["a", "b", "x"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1]);
    app.drop_tab(1, 2);
    assert_eq!(names(&app), ["a", "x", "b"]);
    assert_eq!(groups(&app), [Some(g), None, None]);
}

#[test]
fn a_non_member_dropped_between_two_members_joins() {
    let mut app = app_with(&["a", "b", "x"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1]);
    app.drop_tab(2, 1);
    assert_eq!(names(&app), ["a", "x", "b"]);
    assert_eq!(groups(&app), [Some(g); 3]);
}

#[test]
fn a_dropped_group_block_lands_at_the_slot() {
    let mut app = app_with(&["a", "b", "x", "y"]);
    let g = app.create_group("G".into(), octa::data::MarkColor::Red, &[0, 1]);
    app.drop_group(g, 1);
    assert_eq!(names(&app), ["x", "a", "b", "y"]);
}

#[test]
fn a_tab_dropped_onto_a_loose_tab_starts_a_group_after_it() {
    let mut app = app_with(&["a", "b", "c"]);
    app.drop_onto_tab(0, 2);
    assert_eq!(names(&app), ["b", "c", "a"]);
    let g = app.tabs[1].group.expect("grouped");
    assert_eq!(groups(&app), [None, Some(g), Some(g)]);
    let word = octa::i18n::t("tabgroup.word");
    assert_eq!(app.tab_groups[0].name, format!("{word} 1"));

    let mut app = app_with(&["a", "b", "c"]);
    app.drop_onto_tab(2, 0);
    assert_eq!(names(&app), ["a", "c", "b"], "leftwards too");
}

#[test]
fn a_tab_dropped_onto_a_member_lands_right_after_it() {
    let mut app = app_with(&["a", "b", "c", "d"]);
    let g = app.create_group("G".into(), MarkColor::Blue, &[1, 2]);
    app.drop_onto_tab(3, 1);
    assert_eq!(names(&app), ["a", "b", "d", "c"]);
    assert_eq!(groups(&app), [None, Some(g), Some(g), Some(g)]);
    assert_eq!(app.tab_groups.len(), 1);
}

#[test]
fn a_member_dropped_onto_a_loose_tab_moves_to_a_new_group() {
    let mut app = app_with(&["a", "b", "c"]);
    let g = app.create_group("G".into(), MarkColor::Blue, &[0, 1]);
    app.drop_onto_tab(0, 2);
    assert_eq!(names(&app), ["b", "c", "a"]);
    let new = app.tabs[1].group.expect("grouped");
    assert_ne!(new, g);
    assert_eq!(groups(&app), [Some(g), Some(new), Some(new)]);

    // Its last member gone, a group goes too.
    let mut app = app_with(&["a", "b"]);
    let lone = app.create_group("L".into(), MarkColor::Blue, &[0]);
    app.drop_onto_tab(0, 1);
    assert!(app.tab_groups.iter().all(|x| x.id != lone));
    assert_eq!(app.tab_groups.len(), 1);
}

#[test]
fn dropping_onto_a_tab_keeps_the_active_tab_and_ignores_itself() {
    let mut app = app_with(&["a", "b", "c"]);
    app.active_tab = 2;
    app.drop_onto_tab(0, 1);
    assert_eq!(names(&app), ["b", "a", "c"]);
    assert_eq!(app.active_tab, 2);

    let mut app = app_with(&["a", "b"]);
    app.drop_onto_tab(1, 1);
    app.drop_onto_tab(0, 9);
    assert!(app.tab_groups.is_empty());
    assert_eq!(names(&app), ["a", "b"]);
}
