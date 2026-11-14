use super::*;
use crate::app::state::TabState;

fn app_with_tabs(n: usize) -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs.clear();
    for _ in 0..n {
        app.tabs
            .push(TabState::new(app.settings.default_search_mode));
    }
    app
}

#[test]
fn a_set_shows_from_its_members_and_dissolves_at_one_tab() {
    let mut app = app_with_tabs(3);
    app.tile_tabs(&[0, 2], TileLayout::SideBySide);

    app.active_tab = 0;
    let (set, members) = app.active_tile_members().expect("tab 0 is tiled");
    assert_eq!(members, [0, 2]);
    app.active_tab = 1;
    assert_eq!(app.active_tile_members(), None, "tab 1 is not a member");

    app.close_tab(2);
    assert_eq!(app.tabs[0].tile, None, "a set of one is no set");
    assert!(app.tile_sets.iter().all(|s| s.id != set));
}

#[test]
fn tiling_again_takes_tabs_out_of_their_old_set() {
    let mut app = app_with_tabs(3);
    app.tile_tabs(&[0, 1], TileLayout::Stacked);
    app.tile_tabs(&[1, 2], TileLayout::Grid);

    assert_eq!(app.tabs[0].tile, None, "the old set kept one tab and went");
    assert_eq!(app.tile_sets.len(), 1);
    app.active_tab = 2;
    let (set, members) = app.active_tile_members().expect("tab 2 is tiled");
    assert_eq!(members, [1, 2]);
    assert_eq!(app.tile_sets[0].layout, TileLayout::Grid);

    app.untile(set);
    assert!(app.tabs.iter().all(|t| t.tile.is_none()));
    assert!(app.tile_sets.is_empty());
}

#[test]
fn tile_members_are_the_selection_plus_the_clicked_tab_once_in_order() {
    let sel: std::collections::HashSet<usize> = [3, 1].into_iter().collect();
    assert_eq!(tile_members(&sel, 2), [1, 2, 3]);
    assert_eq!(tile_members(&sel, 3), [1, 3], "already selected: no repeat");
    assert_eq!(
        tile_members(&Default::default(), 4),
        [4],
        "one tab is not a set"
    );
}

#[test]
fn tiling_the_selection_shows_it_and_a_stale_set_of_one_goes() {
    let mut app = app_with_tabs(4);
    app.tab_multi_selection = [2].into_iter().collect();
    app.active_tab = 0;
    app.tile_selected(3, TileLayout::Grid);
    assert_eq!(app.tabs[2].tile, app.tabs[3].tile);
    assert!(app.tabs[2].tile.is_some() && app.tabs[0].tile.is_none());
    assert!(app.tab_multi_selection.is_empty());
    assert!(app.active_tile_members().is_some(), "the set is on screen");

    // A result tab replaced a member without the set noticing.
    app.tabs[3].tile = None;
    app.dissolve_small_tile_sets();
    assert!(app.tabs[2].tile.is_none() && app.tile_sets.is_empty());
}

#[test]
fn untiling_a_group_leaves_other_sets_and_the_group_alone() {
    let mut app = app_with_tabs(4);
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[0, 1]);
    app.tile_tabs(&[0, 1], TileLayout::Grid);
    let other = app.tile_tabs(&[2, 3], TileLayout::SideBySide);

    app.untile_group(g);

    assert_eq!(app.tabs[0].tile, None);
    assert_eq!(app.tabs[1].tile, None);
    assert_eq!(app.tabs[2].tile, Some(other));
    assert_eq!(app.tile_sets.len(), 1);
    assert_eq!(app.tabs[0].group, Some(g));
}

#[test]
fn a_set_shared_with_other_tabs_goes_on_without_the_group() {
    let mut app = app_with_tabs(4);
    let g = app.create_group("G".into(), octa::data::MarkColor::Blue, &[0, 1]);
    let set = app.tile_tabs(&[0, 1, 2, 3], TileLayout::Grid);

    app.untile_group(g);

    assert_eq!(app.tabs[0].tile, None);
    assert_eq!(app.tabs[2].tile, Some(set));
    assert_eq!(app.tabs[3].tile, Some(set));
}
