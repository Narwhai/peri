//! The toolbar driven headlessly: views, sort, group and filter popovers.

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::app::{App, Output, Row, header_key, row_key};
use crate::query::filter::{Conjunction, Node, Operator};
use crate::query::property::Property;
use crate::query::store::Store;
use crate::query::view::Direction;
use crate::view::view;

fn runtime() -> Runtime<App> {
    let mut app = App::new(Output::Silent, Store::memory());
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.on_key = Some(crate::app::on_key);
    rt.poll = Some(crate::app::poll);
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<App>) {
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn type_text(rt: &mut Runtime<App>, text: &str) {
    for c in text.chars() {
        rt.key(KeyEvent { key: Key::Text(c.to_string()), mods: Mods::default() });
    }
    settle(rt);
}

/// Picks `option` from the select keyed `key`, scrolling the listbox to it first.
fn choose(rt: &mut Runtime<App>, key: &str, option: &str) {
    click(rt, &format!("{key}-trigger"));
    let find = |rt: &Runtime<App>| {
        rt.elements()
            .into_iter()
            .find(|e| e.role == "option" && e.label.as_deref() == Some(option) && e.path.contains(&format!("{key}-content")))
            .unwrap_or_else(|| panic!("{key} has no option {option}"))
    };
    let row = find(rt);
    if row.visible[3] < row.rect[3] {
        let [x, y, w, _] = row.rect;
        let list = rt.elements().into_iter().find(|e| e.path.ends_with(&format!("{key}-content"))).expect("the listbox");
        rt.wheel(x + w / 2.0, list.rect[1] + 10.0, -(y - list.rect[1]));
        settle(rt);
    }
    let row = find(rt);
    let [x, y, w, h] = row.visible;
    rt.pointer_down(x + w / 2.0, y + h / 2.0);
    rt.pointer_up(x + w / 2.0, y + h / 2.0);
    settle(rt);
}

fn titles(rt: &Runtime<App>) -> Vec<String> {
    rt.state.filtered.iter().map(|&i| rt.state.tracks[i].title.clone()).collect()
}

#[test]
fn building_a_filter_from_the_popover_narrows_the_list() {
    let mut rt = runtime();
    click(&mut rt, "filter");
    click(&mut rt, "add-filter");
    // A new condition starts as `Genre has …`, matching everything until it has a value.
    assert_eq!(rt.state.filtered.len(), 13);
    click(&mut rt, "cond-0-value");
    type_text(&mut rt, "ambient");
    assert_eq!(rt.state.filtered.len(), 7, "Laraaji and Nils Frahm are ambient");
    choose(&mut rt, "cond-0-prop", "Year");
    assert_eq!(rt.state.filtered.len(), 13, "a year can't be `ambient`, so the filter waits");
    choose(&mut rt, "cond-0-op", "≥");
    click(&mut rt, "cond-0-value");
    rt.update(|s, _| {
        let edit = s.filter_edits.get_mut(&vec![0]).expect("the value editor");
        edit.text.clear();
        edit.anchor = 0;
        edit.focus = 0;
    });
    type_text(&mut rt, "2020");
    let Node::Condition(c) = &rt.state.views.active().filter.children[0] else { panic!("a condition") };
    assert_eq!((c.property, c.op, c.value.as_str()), (Property::Year, Operator::Ge, "2020"));
    assert_eq!(titles(&rt), ["Mary", "Change", "New Jeans", "Super Shy", "ETA", "Cool With You"]);
    assert_eq!(rt.elements().into_iter().find(|e| e.key.as_deref() == Some("filter")).and_then(|e| e.label).as_deref(), Some("1 filter"));
    assert_eq!(result_count(&rt), "6 songs");
}

fn result_count(rt: &Runtime<App>) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some("result-count")).and_then(|e| e.text).expect("the result count")
}

#[test]
fn the_toolbar_counts_the_views_songs_or_albums() {
    let mut rt = runtime();
    assert_eq!(result_count(&rt), "13 songs");
    click(&mut rt, "albums");
    assert_eq!(result_count(&rt), format!("{} albums", rt.state.shown_albums.len()));
}

#[test]
fn play_queues_the_views_songs_in_order_including_collapsed_groups() {
    let mut rt = runtime();
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    click(&mut rt, &header_key(0));
    let order = rt.state.filtered.clone();
    rt.state.shuffle = true;
    click(&mut rt, "play-view");
    assert_eq!(rt.state.current, Some(order[0]), "the view's first song, even though its group is collapsed");
    assert!(!rt.state.shuffle, "Play plays in order");
    let mut played = vec![order[0]];
    for _ in 1..order.len() {
        click(&mut rt, "next");
        played.push(rt.state.current.expect("playing"));
    }
    assert_eq!(played, order, "every song of the view, once, in its order");
}

#[test]
fn play_in_an_album_view_queues_its_albums_songs() {
    let mut rt = runtime();
    click(&mut rt, "albums");
    click(&mut rt, "play-view");
    let first_album = rt.state.shown_albums[0];
    assert_eq!(rt.state.current, rt.state.albums[first_album].tracks.first().copied());
}

#[test]
fn play_is_disabled_while_the_view_is_empty() {
    let mut rt = runtime();
    click(&mut rt, "filter");
    click(&mut rt, "add-filter");
    choose(&mut rt, "cond-0-prop", "Year");
    click(&mut rt, "cond-0-value");
    type_text(&mut rt, "1000");
    choose(&mut rt, "cond-0-op", "<");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert!(rt.state.filtered.is_empty());
    assert_eq!(result_count(&rt), "0 songs");
    assert!(rt.find("play-view").is_some(), "shown, though disabled");
    click(&mut rt, "play-view");
    assert_eq!(rt.state.current, None);
}

#[test]
fn the_views_menu_plays_a_view_without_opening_it() {
    let mut rt = runtime();
    rt.update(|s, _| {
        let mut short = crate::query::view::View::new("Reversed".to_string());
        short.sorts.push(crate::query::view::Sort { property: Property::Title, direction: Direction::Desc });
        s.views.list.push(short);
    });
    let expected = rt.state.view_songs(1);
    click(&mut rt, "views");
    click(&mut rt, "play-view1");
    assert_eq!(rt.state.views.active, 0, "the open view stays open");
    assert_eq!(rt.state.panel, None, "the menu closes");
    assert_eq!(rt.state.current, Some(expected[0]));
    click(&mut rt, "next");
    assert_eq!(rt.state.current, Some(expected[1]), "Next follows the played view, not the open one");
}

#[test]
fn a_narrow_window_shows_the_toolbar_buttons_as_icons_with_their_labels_as_tooltips() {
    let mut rt = runtime();
    let shows_label = |rt: &Runtime<App>| rt.elements().into_iter().any(|e| e.path.contains("sort") && e.key.as_deref() == Some("label"));
    assert!(shows_label(&rt), "980 wide: Sort shows its label");
    rt.set_viewport((720.0, 640.0), 1.0);
    settle(&mut rt);
    assert!(!shows_label(&rt), "720 wide: Sort is an icon");
    let sort = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("sort")).expect("Sort");
    assert_eq!((sort.label.as_deref(), sort.help.as_deref()), (Some("Sort"), Some("Sort")));
    for key in ["play-view", "columns", "sort", "group", "filter"] {
        let [x, _, w, _] = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).expect(key).rect;
        assert!(x + w <= 720.5, "{key} stays in the window");
    }
}

#[test]
fn nested_groups_are_added_switched_and_removed() {
    let mut rt = runtime();
    click(&mut rt, "filter");
    click(&mut rt, "add-group");
    assert!(matches!(&rt.state.views.active().filter.children[0], Node::Group(g) if g.conjunction == Conjunction::Or));
    choose(&mut rt, "conj-0", "None of the following are true");
    click(&mut rt, "cond-0-0-value");
    type_text(&mut rt, "pop");
    assert!(titles(&rt).iter().all(|t| !["New Jeans", "ETA"].contains(&t.as_str())), "none of: genre has pop");
    assert_eq!(rt.state.filtered.len(), 9);
    click(&mut rt, "group-0-remove");
    assert!(rt.state.views.active().filter.children.is_empty());
    assert_eq!(rt.state.filtered.len(), 13);
}

#[test]
fn grouping_shows_sticky_headers_that_collapse_and_next_follows_the_view() {
    let mut rt = runtime();
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    choose(&mut rt, "group-prop", "Genre");
    click(&mut rt, "group-dir");
    assert_eq!(rt.state.views.active().group_by.map(|g| g.direction), Some(Direction::Desc));
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.panel, None, "Escape closes the popover");
    let titles: Vec<&str> = rt.state.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Pop", "New Age", "K-Pop", "Folk", "Electronic", "Ambient"]);

    // Collapse Pop: its songs leave the list, the header stays.
    click(&mut rt, &header_key(0));
    assert!(rt.state.rows.iter().all(|r| !matches!(r, Row::Track { section: Some(0), .. })));
    assert!(rt.find(&header_key(0)).is_some());

    // Play the first New Age song; Next walks the view's order: the rest of New Age, then K-Pop.
    let first = rt.state.sections[1].items[0];
    rt.update(|s, tx| s.play_index(first, tx));
    let mut played = vec![rt.state.tracks[first].title.clone()];
    for _ in 0..3 {
        click(&mut rt, "next");
        played.push(rt.state.tracks[rt.state.current.expect("playing")].title.clone());
    }
    // The K-Pop songs already played in Pop, so the next song is Folk's first.
    assert_eq!(played, ["The Dance #1", "The Dance #2", "Meditation #1", "Mary"], "a song plays once, in its first group");
    assert!(rt.find(&row_key(first, Some(1))).is_some());
}

#[test]
fn sorts_are_added_reordered_and_flipped_from_the_popover() {
    let mut rt = runtime();
    click(&mut rt, "sort");
    click(&mut rt, "add-sort");
    choose(&mut rt, "sort0-prop", "Duration");
    assert_eq!(titles(&rt)[0], "New Jeans", "shortest first");
    click(&mut rt, "sort0-dir");
    assert_eq!(titles(&rt)[0], "Meditation #1", "longest first");
    click(&mut rt, "add-sort");
    choose(&mut rt, "sort1-prop", "Year");
    click(&mut rt, "sort1-up");
    let sorts = &rt.state.views.active().sorts;
    assert_eq!((sorts[0].property, sorts[1].property), (Property::Year, Property::Duration));
    assert_eq!(titles(&rt)[0], "Meditation #1", "1980 first, then the longest of them");
    click(&mut rt, "sort0-remove");
    click(&mut rt, "sort0-remove");
    assert_eq!(rt.state.filtered, (0..13).collect::<Vec<_>>(), "no sorts: library order");
}

#[test]
fn views_are_created_renamed_switched_duplicated_and_deleted() {
    let mut rt = runtime();
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);

    click(&mut rt, "views");
    click(&mut rt, "add-view");
    assert_eq!(rt.state.views.active, 1);
    assert!(rt.state.views.active().group_by.is_none(), "a new view starts flat");
    assert!(rt.state.sections.is_empty());
    // The new view's name is selected for renaming: typing replaces it.
    type_text(&mut rt, "Short songs");
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.views.list[1].name, "Short songs");

    click(&mut rt, "view0");
    assert_eq!(rt.state.views.active, 0);
    assert!(!rt.state.sections.is_empty(), "back to the grouped view");

    click(&mut rt, "views");
    click(&mut rt, "duplicate0");
    assert_eq!(rt.state.views.list.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["All songs", "All songs copy", "Short songs"]);
    assert!(rt.state.views.list[1].group_by.is_some());
    click(&mut rt, "delete0");
    assert_eq!(rt.state.views.list.len(), 2);
    assert_eq!(rt.state.views.active().name, "All songs copy", "deleting the active view moves to its neighbour");
}

#[test]
fn views_save_as_they_change_and_load_on_the_next_launch() {
    let dir = std::env::temp_dir().join(format!("peri-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("prefs.json");
    let mut app = App::new(Output::Silent, Store::at(&file));
    crate::scene_library(&mut app);
    let mut rt = Runtime::new(app, view, (980.0, 640.0), Env::new(true, 1.0));
    rt.frame(0.0);
    click(&mut rt, "group");
    click(&mut rt, "add-group");
    choose(&mut rt, "group-prop", "Year");

    let mut next = App::new(Output::Silent, Store::at(&file));
    crate::scene_library(&mut next);
    assert_eq!(next.views.active().group_by.map(|g| g.property), Some(Property::Year));
    assert_eq!(next.sections.first().map(|s| s.title.as_str()), Some("1980"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn slash_focuses_search_and_then_types_in_it() {
    let mut rt = runtime();
    type_text(&mut rt, "/");
    assert!(rt.state.search.focused, "/ focuses the search field");
    assert_eq!(rt.state.search.text, "", "the / that focused it isn't typed");
    type_text(&mut rt, "a/b");
    assert_eq!(rt.state.search.text, "a/b", "in the field, / types");
}
